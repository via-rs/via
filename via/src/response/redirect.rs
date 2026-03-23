use http::StatusCode;
use http::header::LOCATION;

use crate::error::Error;
use crate::response::Response;

/// A collection of functions used to generate redirect responses.
pub struct Redirect;

impl Redirect {
    /// Returns a response that redirects the client to the specified `location`
    /// with the status code `302 Found`.
    ///
    /// # Errors
    ///
    /// This function may return an error if the provided `location` cannot be
    /// parsed into an HTTP header value.
    pub fn found(location: &str) -> Result<Response, Error> {
        Self::with_status(location, StatusCode::FOUND)
    }

    /// Returns a response that redirects the client to the specified `location`
    /// with the status code `303 See Other`.
    ///
    /// # Errors
    ///
    /// This function may return an error if the provided `location` cannot be
    /// parsed into an HTTP header value.
    pub fn see_other(location: &str) -> Result<Response, Error> {
        Self::with_status(location, StatusCode::SEE_OTHER)
    }

    /// Returns a response that redirects the client to the specified `location`
    /// with the status code `307 Temporary Redirect`.
    ///
    /// # Errors
    ///
    /// This function may return an error if the provided `location` cannot be
    /// parsed into an HTTP header value.
    pub fn temporary(location: &str) -> Result<Response, Error> {
        Self::with_status(location, StatusCode::TEMPORARY_REDIRECT)
    }

    /// Returns a response that redirects the client to the specified `location`
    /// with the status code `308 Permanent Redirect`.
    ///
    /// # Errors
    ///
    /// This function may return an error if the provided `location` cannot be
    /// parsed into an HTTP header value.
    pub fn permanent(location: &str) -> Result<Response, Error> {
        Self::with_status(location, StatusCode::PERMANENT_REDIRECT)
    }

    /// Returns a response that redirects the client to the specified `location`
    /// with the status code `308 Permanent Redirect`.
    ///
    /// # Errors
    ///
    /// This function may return an error if the provided `location` cannot be
    /// parsed into an HTTP header value or if provided `status` would not
    /// result in a redirect.
    pub fn with_status(location: &str, status: StatusCode) -> Result<Response, Error> {
        if !status.is_redirection() {
            if cfg!(debug_assertions) {
                eprintln!("error: redirect status out of range {}", status);
            }

            crate::raise!(500);
        }

        Response::build()
            .status(status)
            .header(LOCATION, location)
            .finish()
    }
}
