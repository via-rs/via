use bytes::Bytes;
use futures_core::Stream;
use http::header::{CONTENT_LENGTH, CONTENT_TYPE, ETAG, LAST_MODIFIED};
use http_body::Frame;
use httpdate::HttpDate;
use std::fs::{File as StdFile, Metadata};
use std::io::{self, ErrorKind, Read};
use std::mem::MaybeUninit;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::fs::File as TokioFile;
use tokio::io::{AsyncRead, ReadBuf};
use tokio::{task, time};

use super::{Finalize, Response, ResponseBuilder};
use crate::error::{BoxError, Error};

/// The base amount of time that the server will wait before
/// attempting to open a file after an error has occurred.
///
const BASE_DELAY_IN_MILLIS: u64 = 100;

const MAX_BUFFER_SIZE: usize = 16 * 1024; // 16KB

/// The amount of times that we'll retry to open a file if in an error occurs.
///
const MAX_ATTEMPTS: u64 = 5;

/// A function pointer used to generate an etag.
///
type GenerateEtag = fn(&Metadata) -> Result<Option<String>, Error>;

/// A specialized response builder used to serve a single file from disk.
///
pub struct File {
    path: PathBuf,
    etag: Option<GenerateEtag>,
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
    Stream(usize, Metadata, TokioFile),
}

/// A stream that wraps the `AsyncRead` impl for `TokioFile`.
///
#[must_use = "streams do nothing unless polled"]
struct FileStream {
    remaining: usize,
    buffer: Vec<MaybeUninit<u8>>,
    file: TokioFile,
}

/// Attempt to open the file and access the metadata at the provided path.
///
/// If the file size is less than `max_alloc_size`, the contents will be
/// eagerly read into memory.
///
async fn open(path: &Path, max_alloc_size: usize) -> Result<Open, Error> {
    let mut attempts = 0;

    loop {
        let path = path.to_owned();
        let delay = BASE_DELAY_IN_MILLIS << attempts;
        let future = task::spawn_blocking(move || {
            let mut std = StdFile::open(path)?;
            let metadata = std.metadata()?;
            let required_cap = match isize::try_from(metadata.len()) {
                Err(e) => return Err(io::Error::new(ErrorKind::FileTooLarge, e)),
                Ok(cap) => cap as usize,
            };

            if required_cap > max_alloc_size {
                let mut file = TokioFile::from_std(std);

                file.set_max_buf_size(MAX_BUFFER_SIZE);
                Ok(Open::Stream(required_cap, metadata, file))
            } else {
                let mut data = Vec::with_capacity(required_cap);

                std.read_to_end(&mut data)?;
                Ok(Open::Eager(metadata, data))
            }
        });

        break match future.await? {
            Ok(open) => Ok(open),
            Err(error) => match error.kind() {
                ErrorKind::IsADirectory | ErrorKind::NotFound | ErrorKind::PermissionDenied => {
                    Err(Error::from_io_error(error))
                }
                _ => {
                    if attempts > MAX_ATTEMPTS {
                        Err(Error::from_io_error(error))
                    } else {
                        time::sleep(Duration::from_millis(delay)).await;
                        attempts += 1;
                        continue;
                    }
                }
            },
        };
    }
}

impl File {
    /// Specify the path at which the file we want to serve is located.
    ///
    pub fn open(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_owned(),
            etag: None,
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
    pub async fn serve(self, max_alloc_size: usize) -> crate::Result {
        match open(&self.path, max_alloc_size).await? {
            Open::Eager(meta, data) => {
                let response = Response::build().header(CONTENT_LENGTH, data.len());
                self.set_headers(&meta, response)?.body(data.into())
            }
            Open::Stream(len, meta, file) => {
                let response = self.set_headers(&meta, Response::build())?;
                FileStream::new(len, file).finalize(response)
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
    fn new(remaining: usize, file: TokioFile) -> Self {
        Self {
            remaining,
            buffer: vec![MaybeUninit::uninit(); MAX_BUFFER_SIZE],
            file,
        }
    }
}

impl Stream for FileStream {
    type Item = Result<Frame<Bytes>, BoxError>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.remaining == 0 {
            return Poll::Ready(None);
        }

        let this = self.get_mut();
        let mut data = ReadBuf::uninit(&mut this.buffer);

        match Pin::new(&mut this.file).poll_read(context, &mut data) {
            Poll::Pending => Poll::Pending,

            Poll::Ready(Ok(())) => {
                let filled = data.filled();
                let len = filled.len();

                if let Some(remaining) = this.remaining.checked_sub(len) {
                    let data = Bytes::copy_from_slice(filled);
                    this.remaining = remaining;
                    this.buffer[..len].fill_with(MaybeUninit::uninit);
                    Poll::Ready(Some(Ok(Frame::data(data))))
                } else {
                    this.remaining = 0;
                    this.buffer.fill_with(MaybeUninit::uninit);
                    Poll::Ready(Some(Err(io::Error::from(ErrorKind::UnexpectedEof).into())))
                }
            }

            Poll::Ready(Err(error)) => {
                this.remaining = 0;
                this.buffer.fill(MaybeUninit::uninit());
                Poll::Ready(Some(Err(error.into())))
            }
        }
    }
}
