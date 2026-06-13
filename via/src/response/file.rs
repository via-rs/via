use bytes::Bytes;
use futures_core::Stream;
use http::header::{CONTENT_LENGTH, CONTENT_TYPE, ETAG, LAST_MODIFIED};
use http_body::Frame;
use httpdate::HttpDate;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::fs::File as TokioFile;
use tokio::io::AsyncReadExt;
use tokio::sync::OwnedSemaphorePermit;
use tokio_util::io::ReaderStream;
// use tokio_util::io::poll_read_buf;

use super::{Finalize, Response, ResponseBuilder};
use crate::{Error, deny};

/// The amount of data that can be buffered in memory when streaming a file.
///
const BUFFER_SIZE: usize = 64 * 1024; // 64KB

/// A function pointer used to generate an etag.
///
type GenerateEtag = fn(&Metadata) -> Result<Option<String>, Error>;

/// A specialized response builder used to serve a single file from disk.
///
pub struct File {
    path: Box<Path>,
    etag: Option<GenerateEtag>,
    permit: Option<OwnedSemaphorePermit>,
    content_type: Option<String>,
    with_last_modified: bool,
}

/// Represents a file that may or may not have been eagerly read into memory.
enum MaybeRead {
    /// The entire contents of the file are ready to be sent to the client.
    Eager(usize, Metadata, Vec<u8>),

    /// The file is too large to read into memory.
    Lazy(u64, Metadata, TokioFile),
}

/// A stream that wraps the `AsyncRead` impl for `TokioFile`.
#[must_use = "streams do nothing unless polled"]
struct FileStream {
    file: ReaderStream<TokioFile>,
    permit: Option<OwnedSemaphorePermit>,
    remaining: u64,
}

/// Attempt to open the file and access the metadata at the provided path.
///
/// If the file size is less than `max_alloc_size`, the contents will be
/// eagerly read into memory.
///
async fn maybe_read(path: impl AsRef<Path>, max_alloc_size: u64) -> Result<MaybeRead, Error> {
    let mut file = open_async(path).await?;
    let metadata = file.metadata().await.map_err(Error::from_io_error)?;
    let capacity = metadata.len();

    if capacity > max_alloc_size {
        if metadata.is_dir() {
            deny!(403, "Forbidden");
        }

        file.set_max_buf_size(BUFFER_SIZE);
        Ok(MaybeRead::Lazy(capacity, metadata, file))
    } else {
        let mut data = Vec::with_capacity(capacity as usize);
        //                                ^^^^^^^^
        // capacity is guaranteed to be < max_alloc_size <= isize::MAX

        file.read_to_end(&mut data).await.map_or_else(
            |error| Err(Error::from_io_error(error)),
            |len| Ok(MaybeRead::Eager(len, metadata, data)),
        )
    }
}

/// Asynchronously open a file. If an error occurs, map it to a via::Error.
#[inline]
async fn open_async(path: impl AsRef<Path>) -> Result<TokioFile, Error> {
    TokioFile::open(path).await.map_err(Error::from_io_error)
}

impl File {
    /// Specify the path at which the file we want to serve is located.
    ///
    pub fn open(path: PathBuf) -> Self {
        Self {
            path: path.into_boxed_path(),
            etag: None,
            permit: None,
            content_type: None,
            with_last_modified: false,
        }
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

    /// Generate an etag by calling the provided function with a reference to
    /// the file's [Metadata].
    ///
    pub fn etag(self, f: GenerateEtag) -> Self {
        Self {
            etag: Some(f),
            ..self
        }
    }

    /// Assign a semaphore permit to the file. This is used to limit the number
    /// of files that are open concurrently.
    ///
    pub fn permit(mut self, permit: OwnedSemaphorePermit) -> Self {
        self.permit = Some(permit);
        self
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
        let mut file = open_async(&self.path).await?;
        let metadata = file.metadata().await.map_err(Error::from_io_error)?;
        let response = self.set_headers(&metadata)?;

        file.set_max_buf_size(BUFFER_SIZE);
        FileStream::new(file, self.permit, metadata.len()).finalize(response)
    }

    /// Respond with the contents of the file.
    ///
    /// If the file is larger than the provided `max_alloc_size` in bytes, it
    /// will be streamed over the socket with chunked transfer encoding.
    ///
    pub async fn serve(mut self, max_alloc_size: u64) -> crate::Result {
        let max_alloc_size = max_alloc_size.min(isize::MAX as u64);

        match maybe_read(&self.path, max_alloc_size).await? {
            MaybeRead::Eager(len, metadata, data) => {
                let response = self.set_headers(&metadata)?;
                self.permit = None;
                response.header(CONTENT_LENGTH, len).body(data.into())
            }
            MaybeRead::Lazy(len, metadata, file) => {
                let response = self.set_headers(&metadata)?;
                FileStream::new(file, self.permit, len).finalize(response)
            }
        }
    }

    fn set_headers(&self, metadata: &Metadata) -> Result<ResponseBuilder, Error> {
        let mut response = Response::build();

        if let Some(mime_type) = self.content_type.as_ref() {
            response = response.header(CONTENT_TYPE, mime_type);
        }

        if let Some(f) = self.etag.as_ref()
            && let Some(etag) = f(metadata)?
        {
            response = response.header(ETAG, etag);
        }

        if self.with_last_modified {
            let last_modified = HttpDate::from(metadata.modified()?);
            response = response.header(LAST_MODIFIED, last_modified.to_string());
        }

        Ok(response)
    }
}

impl FileStream {
    fn new(file: TokioFile, permit: Option<OwnedSemaphorePermit>, remaining: u64) -> Self {
        Self {
            file: ReaderStream::with_capacity(file, BUFFER_SIZE),
            permit,
            remaining,
        }
    }
}

impl Stream for FileStream {
    type Item = Result<Frame<Bytes>, Error>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.file).poll_next(context) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => {
                self.permit = None;
                self.remaining = 0;
                Poll::Ready(None)
            }
            Poll::Ready(Some(Ok(next))) => {
                match self.remaining.checked_sub(next.len() as u64) {
                    Some(0) | None => {
                        self.remaining = 0;
                        // Wake ASAP, we know the stream has ended.
                        context.waker().wake_by_ref();
                    }
                    Some(remaining) => {
                        self.remaining = remaining;
                    }
                }

                Poll::Ready(Some(Ok(Frame::data(next))))
            }
            Poll::Ready(Some(Err(error))) => {
                self.remaining = 0;
                self.permit = None;
                Poll::Ready(Some(Err(Error::from_io_error(error))))
            }
        }
    }
}
