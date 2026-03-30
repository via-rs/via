use http::StatusCode;
use http::header::ALLOW;
use std::borrow::Cow;
use std::fmt::{self, Display, Formatter};

use super::{Error, ErrorSourceRef, Errors};
use crate::middleware::{BoxFuture, Middleware};
use crate::response::{Finalize, Response, ResponseBuilder};
use crate::{Next, Request};

/// Recover from errors that occur in downstream middleware.
///
pub struct Rescue<F> {
    recover: Box<F>,
}

/// Customize how an [`Error`] is converted to a response.
///
pub struct Sanitizer<'a> {
    json: bool,
    error: &'a mut Error,
    message: Option<Cow<'a, str>>,
}

/// Sanitize errors that occur in downstream middleware.
///
pub fn rescue<F>(recover: F) -> Rescue<F>
where
    F: Fn(&mut Sanitizer) + Copy + Send + Sync,
{
    Rescue {
        recover: Box::new(recover),
    }
}

impl<App, F> Middleware<App> for Rescue<F>
where
    F: Fn(&mut Sanitizer) + Copy + Send + Sync + Sized + 'static,
{
    fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture {
        let future = next.call(request);
        let recover = *self.recover;

        Box::pin(async move {
            future.await.or_else(|mut error| {
                let mut sanitizer = Sanitizer::new(&mut error);
                recover(&mut sanitizer);

                let response = Response::build();
                sanitizer.finalize(response).or_else(|residual| {
                    if cfg!(debug_assertions) {
                        eprintln!("warn: a residual error occurred in rescue");
                        eprintln!("{}", residual);
                    }

                    Ok(error.into())
                })
            })
        })
    }
}

impl<'a> Sanitizer<'a> {
    /// Returns a reference to the error source.
    ///
    pub fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.error.source()
    }

    pub fn respond_with_json(&mut self) {
        self.json = true;
    }

    /// Provide a custom message to use for the response generated from this
    /// error.
    ///
    pub fn set_message<T>(&mut self, message: T)
    where
        Cow<'a, str>: From<T>,
    {
        self.message = Some(message.into());
    }

    /// Overrides the HTTP status code of the error.
    ///
    pub fn set_status(&mut self, status: StatusCode) {
        self.error.status = status;
    }

    /// Use the canonical reason of the status code as the error message.
    ///
    pub fn use_canonical_reason(&mut self) {
        self.message = self.status().canonical_reason().map(Cow::Borrowed);
    }

    /// Generate a json response for the error.
    ///
    pub fn use_json(&mut self) {
        self.json = true;
    }

    fn status(&self) -> StatusCode {
        self.error.status
    }
}

impl<'a> Sanitizer<'a> {
    fn new(error: &'a mut Error) -> Self {
        Self {
            json: false,
            error,
            message: None,
        }
    }
}

impl Display for Sanitizer<'_> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(self.error, f)
    }
}

impl Finalize for Sanitizer<'_> {
    fn finalize(mut self, builder: ResponseBuilder) -> Result<Response, Error> {
        let mut builder = builder.status(self.status());

        if let ErrorSourceRef::AllowMethod(error) = self.error.as_source()
            && let Some(allow) = error.allows()
        {
            builder = builder.header(ALLOW, allow);
        }

        if self.json {
            let json = self.message.take().map_or_else(
                || self.error.repr_json(),
                |message| {
                    let mut errors = Errors::new(self.status());
                    errors.push(message);
                    errors
                },
            );

            builder.json(&json)
        } else if let Some(message) = self.message {
            builder.text(message)
        } else {
            builder.text(self.error.to_string())
        }
    }
}
