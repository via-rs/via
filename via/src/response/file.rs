use bytes::Bytes;
use futures_core::Stream;
use http::header::{CONTENT_LENGTH, CONTENT_TYPE, ETAG, LAST_MODIFIED};
use http_body::Frame;
use httpdate::HttpDate;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::fs::{self, File as TokioFile};
use tokio::sync::OwnedSemaphorePermit;
use tokio_util::io::ReaderStream;

use super::{Finalize, Response, ResponseBuilder};
use crate::Error;

/// The maximum amount of data that can be buffered in memory.
///
const MAX_BUFFER_SIZE: usize = 64 * 1024; // 64KB

/// A function pointer used to generate an etag.
///
type GenerateEtag = fn(&Metadata) -> Result<Option<String>, Error>;

/// A specialized response builder used to serve a single file from disk.
///
pub struct File {
    path: PathBuf,
    etag: Option<GenerateEtag>,
    permit: Option<OwnedSemaphorePermit>,
    content_type: Option<String>,
    with_last_modified: bool,
}

/// The possible outcomes from attempting to open a file.
///
enum Open {
    /// The file was small enough to be read in to memory.
    ///
    Eager(Metadata, Vec<u8>),

    /// The file should be streamed over the socket with chunked
    /// `Transfer-Encoding`.
    ///
    Stream(Metadata, TokioFile),
}

/// A stream that wraps the `AsyncRead` impl for `TokioFile`.
///
#[must_use = "streams do nothing unless polled"]
struct FileStream {
    reader: ReaderStream<TokioFile>,
    permit: Option<OwnedSemaphorePermit>,
}

/// Attempt to open the file and access the metadata at the provided path.
///
/// If the file size is less than `max_alloc_size`, the contents will be
/// eagerly read into memory.
///
async fn open(path: &Path, max_alloc_size: u64) -> Result<Open, Error> {
    let metadata = fs::metadata(path).await.map_err(Error::from_io_error)?;

    if metadata.len() > max_alloc_size {
        let mut file = TokioFile::open(path).await.map_err(Error::from_io_error)?;
        file.set_max_buf_size(0); // Buffering occurs in ReaderStream.
        Ok(Open::Stream(metadata, file))
    } else {
        match fs::read(path).await {
            Ok(data) => Ok(Open::Eager(metadata, data)),
            Err(error) => Err(Error::from_io_error(error)),
        }
    }
}

impl File {
    /// Specify the path at which the file we want to serve is located.
    ///
    pub fn open(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_owned(),
            etag: None,
            permit: None,
            content_type: None,
            with_last_modified: false,
        }
    }

    /// Generate an etag by calling the provided function with a reference to
    /// the file's [Metadata].
    ///
    pub fn etag(self, f: GenerateEtag) -> Self {
        Self {
            etag: Some(f),
            ..self
        }
    }

    // Assign a semaphore permit to the file. This is used to limit the number
    // of files that are open concurrently.
    //
    pub fn permit(mut self, permit: OwnedSemaphorePermit) -> Self {
        self.permit = Some(permit);
        self
    }

    /// Set the value of the `Content-Type` header that will be included in the
    /// response.
    ///
    pub fn content_type(self, mime_type: impl AsRef<str>) -> Self {
        Self {
            content_type: Some(mime_type.as_ref().to_owned()),
            ..self
        }
    }

    /// Include a `Last-Modified` header in the response.
    ///
    pub fn with_last_modified(self) -> Self {
        Self {
            with_last_modified: true,
            ..self
        }
    }

    /// Respond with a stream of the file contents in chunks.
    ///
    pub async fn stream(self) -> crate::Result {
        self.serve(0).await
    }

    /// Respond with the contents of the file.
    ///
    /// If the file is larger than the provided `max_alloc_size` in bytes, it
    /// will be streamed over the socket with chunked transfer encoding.
    ///
    pub async fn serve(mut self, max_alloc_size: u64) -> crate::Result {
        match open(&self.path, max_alloc_size.min(isize::MAX as u64)).await? {
            Open::Eager(meta, data) => {
                let response = Response::build().header(CONTENT_LENGTH, data.len());
                self.permit = None;
                self.set_headers(&meta, response)?.body(data.into())
            }
            Open::Stream(meta, file) => {
                let response = self.set_headers(&meta, Response::build())?;
                FileStream::new(self.permit, file).finalize(response)
            }
        }
    }

    fn set_headers(
        &self,
        meta: &Metadata,
        builder: ResponseBuilder,
    ) -> Result<ResponseBuilder, Error> {
        let mut response = builder;

        if let Some(mime_type) = self.content_type.as_ref() {
            response = response.header(CONTENT_TYPE, mime_type);
        }

        if let Some(f) = self.etag.as_ref()
            && let Some(etag) = f(meta)?
        {
            response = response.header(ETAG, etag);
        }

        if self.with_last_modified {
            let last_modified = HttpDate::from(meta.modified()?);
            response = response.header(LAST_MODIFIED, last_modified.to_string());
        }

        Ok(response)
    }
}

impl FileStream {
    fn new(permit: Option<OwnedSemaphorePermit>, file: TokioFile) -> Self {
        Self {
            reader: ReaderStream::with_capacity(file, MAX_BUFFER_SIZE),
            permit,
        }
    }
}

impl Stream for FileStream {
    type Item = Result<Frame<Bytes>, Error>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.reader)
            .poll_next(context)
            .map(|next| match next {
                Some(Ok(chunk)) => Some(Ok(Frame::data(chunk))),
                Some(Err(error)) => {
                    self.permit = None;
                    Some(Err(Error::from_io_error(error)))
                }
                None => {
                    self.permit = None;
                    None
                }
            })
    }
}
