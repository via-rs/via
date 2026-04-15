#![feature(prelude_import)]
//! An async multi-threaded web framework for people who appreciate simplicity.
//!
//! Documentation is sparse at the moment, but the code is well-commented for
//! the most part.
//!
//! If you're interested in contributing, helping with documentation is a great
//! starting point.
//!
//! ## Hello World Example
//!
//! Below is a basic example to demonstrate how to use Via to create a simple
//! web server that responds to requests at `/hello/:name` with a personalized
//! greeting.
//! [Additional examples](https://github.com/zacharygolba/via/tree/main/examples)
//! can be found in our git repository.
//!
//! ```no_run
//! use std::process::ExitCode;
//! use via::{Error, Next, Request, Response, ResultExt, Server};
//!
//! async fn hello(request: Request, _: Next) -> via::Result {
//!     // Get a reference to the path parameter `name` from the request uri.
//!     let name = request.param("name").percent_decode().into_result()?;
//!
//!     // Send a plain text response with our greeting message.
//!     Response::build().text(format!("Hello, {}!", name))
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<ExitCode, Error> {
//!     let mut app = via::app(());
//!
//!     // Define a route that listens on /hello/:name.
//!     app.route("/hello/:name").to(via::get(hello));
//!
//!     Server::new(app).listen(("127.0.0.1", 8080)).await
//! }
//! ```
//!
extern crate std;
#[prelude_import]
use std::prelude::rust_2024::*;
pub mod error {
    //! Error handling.
    //!
    mod raise {}
    mod rescue {
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
                    future
                        .await
                        .or_else(|mut error| {
                            let mut sanitizer = Sanitizer::new(&mut error);
                            recover(&mut sanitizer);
                            let response = Response::build();
                            sanitizer
                                .finalize(response)
                                .or_else(|residual| {
                                    if true {
                                        {
                                            ::std::io::_eprint(
                                                format_args!("warn: a residual error occurred in rescue\n"),
                                            );
                                        };
                                        {
                                            ::std::io::_eprint(format_args!("{0}\n", residual));
                                        };
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
            fn finalize(self, builder: ResponseBuilder) -> Result<Response, Error> {
                let mut builder = builder.status(self.status());
                if let ErrorSourceRef::AllowMethod(error) = self.error.as_source()
                    && let Some(allow) = error.allows()
                {
                    builder = builder.header(ALLOW, allow);
                }
                if self.json {
                    let json = self
                        .message
                        .as_deref()
                        .map_or_else(
                            || self.error.repr_json(),
                            |message| Errors::new(self.status(), message),
                        );
                    builder.json(&json)
                } else if let Some(message) = self.message {
                    builder.text(message)
                } else {
                    builder.text(self.error.to_string())
                }
            }
        }
    }
    mod result {
        use http::StatusCode;
        use super::Error;
        pub trait ResultExt: Sized {
            type Output;
            fn into_result(self) -> Result<Self::Output, Error>;
            fn or_bad_request(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::BAD_REQUEST))
            }
            fn or_unauthorized(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::UNAUTHORIZED))
            }
            fn or_payment_required(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::PAYMENT_REQUIRED))
            }
            fn or_forbidden(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::FORBIDDEN))
            }
            fn or_not_found(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::NOT_FOUND))
            }
            fn or_method_not_allowed(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::METHOD_NOT_ALLOWED))
            }
            fn or_not_acceptable(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::NOT_ACCEPTABLE))
            }
            fn or_proxy_authentication_required(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(
                        error,
                        StatusCode::PROXY_AUTHENTICATION_REQUIRED,
                    ))
            }
            fn or_request_timeout(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::REQUEST_TIMEOUT))
            }
            fn or_conflict(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::CONFLICT))
            }
            fn or_gone(self) -> Result<Self::Output, Error> {
                self.into_result().map_err(|error| set_status(error, StatusCode::GONE))
            }
            fn or_length_required(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::LENGTH_REQUIRED))
            }
            fn or_precondition_failed(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::PRECONDITION_FAILED))
            }
            fn or_payload_too_large(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::PAYLOAD_TOO_LARGE))
            }
            fn or_uri_too_long(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::URI_TOO_LONG))
            }
            fn or_unsupported_media_type(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(
                        error,
                        StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    ))
            }
            fn or_range_not_satisfiable(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(
                        error,
                        StatusCode::RANGE_NOT_SATISFIABLE,
                    ))
            }
            fn or_expectation_failed(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::EXPECTATION_FAILED))
            }
            fn or_im_a_teapot(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::IM_A_TEAPOT))
            }
            fn or_misdirected_request(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::MISDIRECTED_REQUEST))
            }
            fn or_unprocessable_entity(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::UNPROCESSABLE_ENTITY))
            }
            fn or_locked(self) -> Result<Self::Output, Error> {
                self.into_result().map_err(|error| set_status(error, StatusCode::LOCKED))
            }
            fn or_failed_dependency(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::FAILED_DEPENDENCY))
            }
            fn or_upgrade_required(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::UPGRADE_REQUIRED))
            }
            fn or_precondition_required(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(
                        error,
                        StatusCode::PRECONDITION_REQUIRED,
                    ))
            }
            fn or_too_many_requests(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::TOO_MANY_REQUESTS))
            }
            fn or_request_header_fields_too_large(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(
                        error,
                        StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
                    ))
            }
            fn or_unavailable_for_legal_reasons(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(
                        error,
                        StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS,
                    ))
            }
            fn or_internal_server_error(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(
                        error,
                        StatusCode::INTERNAL_SERVER_ERROR,
                    ))
            }
            fn or_not_implemented(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::NOT_IMPLEMENTED))
            }
            fn or_bad_gateway(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::BAD_GATEWAY))
            }
            fn or_service_unavailable(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::SERVICE_UNAVAILABLE))
            }
            fn or_gateway_timeout(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::GATEWAY_TIMEOUT))
            }
            fn or_http_version_not_supported(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(
                        error,
                        StatusCode::HTTP_VERSION_NOT_SUPPORTED,
                    ))
            }
            fn or_variant_also_negotiates(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(
                        error,
                        StatusCode::VARIANT_ALSO_NEGOTIATES,
                    ))
            }
            fn or_insufficient_storage(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::INSUFFICIENT_STORAGE))
            }
            fn or_loop_detected(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::LOOP_DETECTED))
            }
            fn or_not_extended(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(error, StatusCode::NOT_EXTENDED))
            }
            fn or_network_authentication_required(self) -> Result<Self::Output, Error> {
                self.into_result()
                    .map_err(|error| set_status(
                        error,
                        StatusCode::NETWORK_AUTHENTICATION_REQUIRED,
                    ))
            }
        }
        const fn set_status(mut error: Error, status: StatusCode) -> Error {
            error.status = status;
            error
        }
        impl<T, E> ResultExt for Result<T, E>
        where
            Error: From<E>,
        {
            type Output = T;
            #[inline]
            fn into_result(self) -> Result<Self::Output, Error> {
                self.map_err(Error::from)
            }
        }
    }
    mod server {
        use std::error::Error;
        use std::fmt::{self, Debug, Display, Formatter};
        use std::io;
        use tokio::time::error::Elapsed;
        use super::BoxError;
        struct HandshakeTimeoutError;
        #[automatically_derived]
        impl ::core::fmt::Debug for HandshakeTimeoutError {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::write_str(f, "HandshakeTimeoutError")
            }
        }
        pub(crate) enum ServerError {
            Http(hyper::Error),
            Other(BoxError),
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for ServerError {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                match self {
                    ServerError::Http(__self_0) => {
                        ::core::fmt::Formatter::debug_tuple_field1_finish(
                            f,
                            "Http",
                            &__self_0,
                        )
                    }
                    ServerError::Other(__self_0) => {
                        ::core::fmt::Formatter::debug_tuple_field1_finish(
                            f,
                            "Other",
                            &__self_0,
                        )
                    }
                }
            }
        }
        impl Error for HandshakeTimeoutError {}
        impl Display for HandshakeTimeoutError {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_fmt(
                    format_args!(
                        "tls negotiation did not finish within the configured timeout\n",
                    ),
                )
            }
        }
        impl Display for ServerError {
            fn fmt(&self, f: &mut Formatter) -> fmt::Result {
                match self {
                    Self::Http(error) => Display::fmt(error, f),
                    Self::Other(error) => Display::fmt(&**error, f),
                }
            }
        }
        impl Error for ServerError {
            fn source(&self) -> Option<&(dyn Error + 'static)> {
                match self {
                    Self::Http(error) => error.source(),
                    Self::Other(error) => Error::source(&**error),
                }
            }
        }
        impl From<Elapsed> for ServerError {
            fn from(_: Elapsed) -> Self {
                Self::Other(Box::new(HandshakeTimeoutError))
            }
        }
        impl From<hyper::Error> for ServerError {
            fn from(error: hyper::Error) -> Self {
                Self::Http(error)
            }
        }
        impl From<io::Error> for ServerError {
            fn from(error: io::Error) -> Self {
                Self::Other(Box::new(error))
            }
        }
    }
    use http::header::{CONTENT_LENGTH, CONTENT_TYPE};
    use serde::{Serialize, Serializer};
    use std::fmt::{self, Debug, Display, Formatter};
    use std::io::{self, Error as IoError};
    #[doc(hidden)]
    pub use http::StatusCode;
    pub use rescue::{Rescue, Sanitizer, rescue};
    pub use result::ResultExt;
    pub(crate) use server::ServerError;
    use crate::response::Response;
    use crate::router::MethodNotAllowed;
    /// A type alias for `Box<dyn Error + Send + Sync>`.
    ///
    pub type BoxError = Box<dyn std::error::Error + Send + Sync>;
    /// An error type that can act as a specialized version of a
    /// [`ResponseBuilder`](crate::response::ResponseBuilder).
    ///
    pub struct Error {
        status: StatusCode,
        source: ErrorSource,
    }
    #[automatically_derived]
    impl ::core::fmt::Debug for Error {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            ::core::fmt::Formatter::debug_struct_field2_finish(
                f,
                "Error",
                "status",
                &self.status,
                "source",
                &&self.source,
            )
        }
    }
    enum ErrorSource {
        AllowMethod(Box<MethodNotAllowed>),
        Message(String),
        Other(BoxError),
        Json(serde_json::Error),
    }
    #[automatically_derived]
    impl ::core::fmt::Debug for ErrorSource {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            match self {
                ErrorSource::AllowMethod(__self_0) => {
                    ::core::fmt::Formatter::debug_tuple_field1_finish(
                        f,
                        "AllowMethod",
                        &__self_0,
                    )
                }
                ErrorSource::Message(__self_0) => {
                    ::core::fmt::Formatter::debug_tuple_field1_finish(
                        f,
                        "Message",
                        &__self_0,
                    )
                }
                ErrorSource::Other(__self_0) => {
                    ::core::fmt::Formatter::debug_tuple_field1_finish(
                        f,
                        "Other",
                        &__self_0,
                    )
                }
                ErrorSource::Json(__self_0) => {
                    ::core::fmt::Formatter::debug_tuple_field1_finish(
                        f,
                        "Json",
                        &__self_0,
                    )
                }
            }
        }
    }
    enum ErrorSourceRef<'a> {
        AllowMethod(&'a MethodNotAllowed),
        Message(&'a str),
        Other(&'a (dyn std::error::Error + 'static)),
        Json(&'a serde_json::Error),
    }
    #[serde(untagged)]
    enum ErrorList<'a> {
        Original(&'a str),
        Chain(Vec<String>),
    }
    #[doc(hidden)]
    #[allow(
        non_upper_case_globals,
        unused_attributes,
        unused_qualifications,
        clippy::absolute_paths,
    )]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl<'a> _serde::Serialize for ErrorList<'a> {
            fn serialize<__S>(
                &self,
                __serializer: __S,
            ) -> _serde::__private228::Result<__S::Ok, __S::Error>
            where
                __S: _serde::Serializer,
            {
                match *self {
                    ErrorList::Original(ref __field0) => {
                        _serde::Serialize::serialize(__field0, __serializer)
                    }
                    ErrorList::Chain(ref __field0) => {
                        _serde::Serialize::serialize(__field0, __serializer)
                    }
                }
            }
        }
    };
    struct Errors<'a> {
        #[serde(serialize_with = "serialize_status_code")]
        status: StatusCode,
        errors: ErrorList<'a>,
    }
    #[doc(hidden)]
    #[allow(
        non_upper_case_globals,
        unused_attributes,
        unused_qualifications,
        clippy::absolute_paths,
    )]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl<'a> _serde::Serialize for Errors<'a> {
            fn serialize<__S>(
                &self,
                __serializer: __S,
            ) -> _serde::__private228::Result<__S::Ok, __S::Error>
            where
                __S: _serde::Serializer,
            {
                let mut __serde_state = _serde::Serializer::serialize_struct(
                    __serializer,
                    "Errors",
                    false as usize + 1 + 1,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "status",
                    &{
                        #[doc(hidden)]
                        struct __SerializeWith<'__a, 'a: '__a> {
                            values: (&'__a StatusCode,),
                            phantom: _serde::__private228::PhantomData<Errors<'a>>,
                        }
                        #[automatically_derived]
                        impl<'__a, 'a: '__a> _serde::Serialize
                        for __SerializeWith<'__a, 'a> {
                            fn serialize<__S>(
                                &self,
                                __s: __S,
                            ) -> _serde::__private228::Result<__S::Ok, __S::Error>
                            where
                                __S: _serde::Serializer,
                            {
                                serialize_status_code(self.values.0, __s)
                            }
                        }
                        __SerializeWith {
                            values: (&self.status,),
                            phantom: _serde::__private228::PhantomData::<Errors<'a>>,
                        }
                    },
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "errors",
                    &self.errors,
                )?;
                _serde::ser::SerializeStruct::end(__serde_state)
            }
        }
    };
    pub fn deny<S, M>(status: S, message: M) -> Error
    where
        S: TryInto<StatusCode>,
        S::Error: std::error::Error + Send + Sync + 'static,
        M: Into<String>,
    {
        match status.try_into() {
            Err(error) => Error::other(Box::new(error)),
            Ok(status) => {
                let source = ErrorSource::Message(message.into());
                Error { status, source }
            }
        }
    }
    fn serialize_status_code<S>(
        status: &StatusCode,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u16(status.as_u16())
    }
    impl Error {
        /// Returns a new error with the provided status and message.
        ///
        pub fn new(message: impl Into<String>) -> Self {
            Self {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                source: ErrorSource::Message(message.into()),
            }
        }
        pub fn other(source: BoxError) -> Self {
            Self {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                source: ErrorSource::Other(source),
            }
        }
        /// Returns a new error with the provided source a status code derived from
        /// the [`ErrorKind`](io::ErrorKind).
        ///
        pub fn from_io_error(error: IoError) -> Self {
            let status = match error.kind() {
                io::ErrorKind::AlreadyExists => StatusCode::CONFLICT,
                io::ErrorKind::BrokenPipe
                | io::ErrorKind::ConnectionReset
                | io::ErrorKind::ConnectionAborted => StatusCode::BAD_GATEWAY,
                io::ErrorKind::ConnectionRefused => StatusCode::SERVICE_UNAVAILABLE,
                io::ErrorKind::InvalidData | io::ErrorKind::InvalidInput => {
                    StatusCode::BAD_REQUEST
                }
                io::ErrorKind::IsADirectory
                | io::ErrorKind::NotADirectory
                | io::ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
                io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
                io::ErrorKind::TimedOut => StatusCode::GATEWAY_TIMEOUT,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            Self::other_with_status(Box::new(error), status)
        }
        /// Returns a reference to the error source.
        ///
        pub fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            match self.as_source() {
                ErrorSourceRef::AllowMethod(source) => Some(source),
                ErrorSourceRef::Other(source) => Some(source),
                ErrorSourceRef::Json(source) => Some(source),
                _ => None,
            }
        }
        pub fn status(&self) -> StatusCode {
            self.status
        }
    }
    impl Error {
        pub(crate) fn invalid_utf8_sequence(name: &str) -> Self {
            let mut error = Self::new(
                ::alloc::__export::must_use({
                    ::alloc::fmt::format(
                        format_args!("invalid utf-8 sequence of bytes in {0}.", name),
                    )
                }),
            );
            error.status = StatusCode::BAD_REQUEST;
            error
        }
        pub(crate) fn method_not_allowed(error: MethodNotAllowed) -> Self {
            Self {
                source: ErrorSource::AllowMethod(Box::new(error)),
                status: StatusCode::METHOD_NOT_ALLOWED,
            }
        }
        pub(crate) fn payload_too_large() -> Self {
            let message = "request body exceeds the maximum length".to_owned();
            Self {
                source: ErrorSource::Message(message),
                status: StatusCode::PAYLOAD_TOO_LARGE,
            }
        }
        pub(crate) fn require_path_param(name: &str) -> Self {
            let mut error = Self::new(
                ::alloc::__export::must_use({
                    ::alloc::fmt::format(
                        format_args!("missing required path parameter: \"{0}\".", name),
                    )
                }),
            );
            error.status = StatusCode::BAD_REQUEST;
            error
        }
        pub(crate) fn require_query_param(name: &str) -> Self {
            let mut error = Self::new(
                ::alloc::__export::must_use({
                    ::alloc::fmt::format(
                        format_args!("missing required query parameter: \"{0}\".", name),
                    )
                }),
            );
            error.status = StatusCode::BAD_REQUEST;
            error
        }
        pub(crate) fn ser_json(source: serde_json::Error) -> Self {
            Self {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                source: ErrorSource::Json(source),
            }
        }
        pub(crate) fn de_json(source: serde_json::Error) -> Self {
            Self {
                status: StatusCode::BAD_REQUEST,
                source: ErrorSource::Json(source),
            }
        }
        #[doc(hidden)]
        pub fn other_with_status(source: BoxError, status: StatusCode) -> Self {
            let mut error = Self::other(source);
            error.status = status;
            error
        }
        #[inline]
        fn as_source(&self) -> ErrorSourceRef<'_> {
            match &self.source {
                ErrorSource::AllowMethod(source) => ErrorSourceRef::AllowMethod(source),
                ErrorSource::Message(message) => ErrorSourceRef::Message(message),
                ErrorSource::Other(source) => ErrorSourceRef::Other(source.as_ref()),
                ErrorSource::Json(source) => ErrorSourceRef::Json(source),
            }
        }
        fn repr_json(&self) -> Errors<'_> {
            if let ErrorSourceRef::Message(message) = self.as_source() {
                Errors::new(self.status, message)
            } else {
                let mut errors = Vec::with_capacity(12);
                let mut source = self.source();
                while let Some(error) = source {
                    errors.push(error.to_string());
                    source = error.source();
                }
                errors.reverse();
                Errors::chain(self.status, errors)
            }
        }
    }
    impl Display for Error {
        fn fmt(&self, f: &mut Formatter) -> fmt::Result {
            match self.as_source() {
                ErrorSourceRef::Message(message) => Display::fmt(message, f),
                ErrorSourceRef::AllowMethod(source) => Display::fmt(source, f),
                ErrorSourceRef::Other(source) => Display::fmt(source, f),
                ErrorSourceRef::Json(source) => Display::fmt(source, f),
            }
        }
    }
    impl From<Error> for BoxError {
        fn from(error: Error) -> Self {
            match error.source {
                ErrorSource::AllowMethod(source) => source,
                ErrorSource::Message(string) => string.into(),
                ErrorSource::Other(source) => source,
                ErrorSource::Json(source) => source.into(),
            }
        }
    }
    impl<E> From<E> for Error
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        fn from(source: E) -> Self {
            Self::other(Box::new(source))
        }
    }
    impl From<Error> for Response {
        fn from(error: Error) -> Self {
            let message = error.to_string();
            let content_len = message.len().into();
            let mut response = Self::new(message.into());
            *response.status_mut() = error.status;
            let headers = response.headers_mut();
            headers.insert(CONTENT_LENGTH, content_len);
            if let Ok(content_type) = "text/plain; charset=utf-8".try_into() {
                headers.insert(CONTENT_TYPE, content_type);
            }
            response
        }
    }
    impl Serialize for Error {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            self.repr_json().serialize(serializer)
        }
    }
    impl<'a> Errors<'a> {
        fn new(status: StatusCode, message: &'a str) -> Self {
            Self {
                status,
                errors: ErrorList::Original(message),
            }
        }
        fn chain(status: StatusCode, chain: Vec<String>) -> Self {
            Self {
                status,
                errors: ErrorList::Chain(chain),
            }
        }
    }
}
pub mod guard {
    pub mod header {
        mod sequence {
            use crate::guard::{ErrorKind, Not, Predicate, not};
            pub struct Contains<T>(Has<Comma, T>);
            pub struct Token<T>(Has<Not<Tchar>, T>);
            struct Comma;
            struct Tchar;
            struct Has<T, U> {
                separator: T,
                predicate: U,
            }
            /// Succeeds if `predicate` matches a comma separated value in the header.
            pub fn contains<T>(predicate: T) -> Contains<T> {
                Contains(Has { separator: Comma, predicate })
            }
            /// Succeeds if `predicate` matches one of the tokens in the header value.
            pub fn token<T>(predicate: T) -> Token<T> {
                Token(Has {
                    separator: not(Tchar),
                    predicate,
                })
            }
            impl Predicate<u8> for Comma {
                fn cmp(&self, byte: &u8) -> Result<(), ErrorKind> {
                    if *byte == b',' { Ok(()) } else { Err(ErrorKind::Match) }
                }
            }
            impl Predicate<u8> for Tchar {
                fn cmp(&self, byte: &u8) -> Result<(), ErrorKind> {
                    match byte {
                        0x21
                        | 0x23..=0x27
                        | 0x2A
                        | 0x2B
                        | 0x2D
                        | 0x2E
                        | 0x30..=0x39
                        | 0x41..=0x5A
                        | 0x5E..=0x7A
                        | 0x7C
                        | 0x7E => Ok(()),
                        _ => Err(ErrorKind::Match),
                    }
                }
            }
            impl<T> Predicate<[u8]> for Contains<T>
            where
                T: Predicate<[u8]>,
            {
                fn cmp(&self, value: &[u8]) -> Result<(), ErrorKind> {
                    self.0.cmp(value)
                }
            }
            impl<T, U> Predicate<[u8]> for Has<T, U>
            where
                T: Predicate<u8>,
                U: Predicate<[u8]>,
            {
                fn cmp(&self, value: &[u8]) -> Result<(), ErrorKind> {
                    if value
                        .split(|byte| self.separator.matches(byte))
                        .any(|item| self.predicate.matches(item.trim_ascii()))
                    {
                        Ok(())
                    } else {
                        Err(ErrorKind::Match)
                    }
                }
            }
        }
        mod tag {
            use super::Predicate;
            use crate::guard::{ErrorKind, Or, or};
            pub type ApplicationJson = Or<(Tag, Tag, Tag)>;
            pub struct CaseSensitive(Box<[u8]>);
            pub struct StartsWith(Box<[u8]>);
            pub struct EndsWith(Box<[u8]>);
            pub struct Tag(Box<[u8]>);
            pub fn case_sensitive(value: &[u8]) -> CaseSensitive {
                CaseSensitive(value.to_owned().into_boxed_slice())
            }
            pub fn starts_with(prefix: &[u8]) -> StartsWith {
                StartsWith(prefix.to_owned().into_boxed_slice())
            }
            pub fn ends_with(suffix: &[u8]) -> EndsWith {
                EndsWith(suffix.to_owned().into_boxed_slice())
            }
            pub fn tag(value: &[u8]) -> Tag {
                Tag(value.to_owned().into_boxed_slice())
            }
            impl Predicate<[u8]> for CaseSensitive {
                fn cmp(&self, value: &[u8]) -> Result<(), ErrorKind> {
                    if &*self.0 == value { Ok(()) } else { Err(ErrorKind::Match) }
                }
            }
            impl Predicate<[u8]> for StartsWith {
                fn cmp(&self, prefix: &[u8]) -> Result<(), ErrorKind> {
                    if prefix.starts_with(&self.0) {
                        Ok(())
                    } else {
                        Err(ErrorKind::Match)
                    }
                }
            }
            impl Predicate<[u8]> for EndsWith {
                fn cmp(&self, suffix: &[u8]) -> Result<(), ErrorKind> {
                    if suffix.ends_with(&self.0) {
                        Ok(())
                    } else {
                        Err(ErrorKind::Match)
                    }
                }
            }
            impl Predicate<[u8]> for Tag {
                fn cmp(&self, value: &[u8]) -> Result<(), ErrorKind> {
                    if (*self.0).eq_ignore_ascii_case(value) {
                        Ok(())
                    } else {
                        Err(ErrorKind::Match)
                    }
                }
            }
            pub(super) fn application_json() -> ApplicationJson {
                or((
                    tag(b"application/json"),
                    tag(b"application/json; charset=utf-8"),
                    tag(b"application/json;charset=utf-8"),
                ))
            }
        }
        pub mod accept {
            use http::header::ACCEPT;
            use super::sequence::{Contains, contains};
            use super::tag::{self, ApplicationJson, CaseSensitive, case_sensitive};
            use super::{Header, header};
            use crate::guard::predicate::{Or, or};
            pub type Accept<T> = Contains<Or<(CaseSensitive, T)>>;
            pub fn accept<T>(predicate: T) -> Header<Accept<T>> {
                header(ACCEPT, contains(or((case_sensitive(b"*/*"), predicate))))
            }
            pub fn json() -> Header<Accept<ApplicationJson>> {
                accept(tag::application_json())
            }
        }
        pub mod content_type {
            use http::header::CONTENT_TYPE;
            use super::tag::{ApplicationJson, application_json};
            use super::{Header, header};
            pub fn content_type<T>(predicate: T) -> Header<T> {
                header(CONTENT_TYPE, predicate)
            }
            pub fn json() -> Header<ApplicationJson> {
                content_type(application_json())
            }
        }
        pub use accept::accept;
        pub use content_type::content_type;
        pub use sequence::*;
        pub use tag::*;
        use http::{HeaderMap, header::HeaderName};
        use std::fmt::Debug;
        use super::{ErrorKind, Predicate};
        use crate::Request;
        pub struct Header<T> {
            optional: bool,
            value: T,
            key: HeaderName,
        }
        pub fn header<K, V>(key: K, value: V) -> Header<V>
        where
            K: TryInto<HeaderName>,
            K::Error: Debug,
        {
            Header {
                optional: false,
                value,
                key: key.try_into().expect("invalid header name."),
            }
        }
        impl<T> Header<T> {
            pub fn optional(mut self) -> Self {
                self.optional = true;
                self
            }
        }
        impl<T> Predicate<HeaderMap> for Header<T>
        where
            T: Predicate<[u8]>,
        {
            fn cmp(&self, headers: &HeaderMap) -> Result<(), ErrorKind> {
                match headers.get(&self.key) {
                    Some(value) if self.value.cmp(value.as_bytes()).is_ok() => Ok(()),
                    None if self.optional => Ok(()),
                    _ => Err(ErrorKind::Header(self.key.clone())),
                }
            }
        }
        impl<T, App> Predicate<Request<App>> for Header<T>
        where
            T: Predicate<[u8]>,
        {
            fn cmp(&self, request: &Request<App>) -> Result<(), ErrorKind> {
                match request.headers().get(&self.key) {
                    Some(value) if self.value.cmp(value.as_bytes()).is_ok() => Ok(()),
                    None if self.optional => Ok(()),
                    _ => Err(ErrorKind::Header(self.key.clone())),
                }
            }
        }
    }
    pub mod method {
        use super::error::ErrorKind;
        use super::predicate::{Not, Predicate, not};
        use crate::request::Request;
        pub struct IsSafe;
        pub fn is_mutation() -> Not<IsSafe> {
            not(is_safe())
        }
        pub fn is_safe() -> IsSafe {
            IsSafe
        }
        impl<App> Predicate<Request<App>> for IsSafe {
            fn cmp(&self, request: &Request<App>) -> Result<(), ErrorKind> {
                if request.method().is_safe() { Ok(()) } else { Err(ErrorKind::Method) }
            }
        }
    }
    mod error {
        use http::HeaderName;
        use crate::Error;
        pub enum ErrorKind {
            Header(HeaderName),
            Match,
            Method,
            Not,
            Other(Error),
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for ErrorKind {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                match self {
                    ErrorKind::Header(__self_0) => {
                        ::core::fmt::Formatter::debug_tuple_field1_finish(
                            f,
                            "Header",
                            &__self_0,
                        )
                    }
                    ErrorKind::Match => ::core::fmt::Formatter::write_str(f, "Match"),
                    ErrorKind::Method => ::core::fmt::Formatter::write_str(f, "Method"),
                    ErrorKind::Not => ::core::fmt::Formatter::write_str(f, "Not"),
                    ErrorKind::Other(__self_0) => {
                        ::core::fmt::Formatter::debug_tuple_field1_finish(
                            f,
                            "Other",
                            &__self_0,
                        )
                    }
                }
            }
        }
    }
    mod predicate {
        use super::ErrorKind;
        pub struct And<T>(T);
        pub struct Or<T>(T);
        pub struct Not<T>(T);
        pub struct When<T, U> {
            condition: T,
            predicate: U,
        }
        pub trait Predicate<Input: ?Sized> {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind>;
            fn matches(&self, input: &Input) -> bool {
                self.cmp(input).is_ok()
            }
        }
        pub fn and<T>(list: T) -> And<T> {
            And(list)
        }
        pub fn not<T>(predicate: T) -> Not<T> {
            Not(predicate)
        }
        pub fn or<T>(list: T) -> Or<T> {
            Or(list)
        }
        pub fn when<T, U>(condition: T, predicate: U) -> When<T, U> {
            When { condition, predicate }
        }
        impl<Input, A, B> Predicate<Input> for And<(A, B)>
        where
            A: Predicate<Input>,
            B: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B) = &self.0;
                A.cmp(input).and_then(|_| B.cmp(input))
            }
        }
        impl<Input, A, B, C> Predicate<Input> for And<(A, B, C)>
        where
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C) = &self.0;
                A.cmp(input).and_then(|_| B.cmp(input)).and_then(|_| C.cmp(input))
            }
        }
        impl<Input, A, B, C, D> Predicate<Input> for And<(A, B, C, D)>
        where
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D) = &self.0;
                A.cmp(input)
                    .and_then(|_| B.cmp(input))
                    .and_then(|_| C.cmp(input))
                    .and_then(|_| D.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E> Predicate<Input> for And<(A, B, C, D, E)>
        where
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E) = &self.0;
                A.cmp(input)
                    .and_then(|_| B.cmp(input))
                    .and_then(|_| C.cmp(input))
                    .and_then(|_| D.cmp(input))
                    .and_then(|_| E.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F> Predicate<Input> for And<(A, B, C, D, E, F)>
        where
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F) = &self.0;
                A.cmp(input)
                    .and_then(|_| B.cmp(input))
                    .and_then(|_| C.cmp(input))
                    .and_then(|_| D.cmp(input))
                    .and_then(|_| E.cmp(input))
                    .and_then(|_| F.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G> Predicate<Input> for And<(A, B, C, D, E, F, G)>
        where
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G) = &self.0;
                A.cmp(input)
                    .and_then(|_| B.cmp(input))
                    .and_then(|_| C.cmp(input))
                    .and_then(|_| D.cmp(input))
                    .and_then(|_| E.cmp(input))
                    .and_then(|_| F.cmp(input))
                    .and_then(|_| G.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G, H> Predicate<Input>
        for And<(A, B, C, D, E, F, G, H)>
        where
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H) = &self.0;
                A.cmp(input)
                    .and_then(|_| B.cmp(input))
                    .and_then(|_| C.cmp(input))
                    .and_then(|_| D.cmp(input))
                    .and_then(|_| E.cmp(input))
                    .and_then(|_| F.cmp(input))
                    .and_then(|_| G.cmp(input))
                    .and_then(|_| H.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G, H, I> Predicate<Input>
        for And<(A, B, C, D, E, F, G, H, I)>
        where
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I) = &self.0;
                A.cmp(input)
                    .and_then(|_| B.cmp(input))
                    .and_then(|_| C.cmp(input))
                    .and_then(|_| D.cmp(input))
                    .and_then(|_| E.cmp(input))
                    .and_then(|_| F.cmp(input))
                    .and_then(|_| G.cmp(input))
                    .and_then(|_| H.cmp(input))
                    .and_then(|_| I.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G, H, I, J> Predicate<Input>
        for And<(A, B, C, D, E, F, G, H, I, J)>
        where
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J) = &self.0;
                A.cmp(input)
                    .and_then(|_| B.cmp(input))
                    .and_then(|_| C.cmp(input))
                    .and_then(|_| D.cmp(input))
                    .and_then(|_| E.cmp(input))
                    .and_then(|_| F.cmp(input))
                    .and_then(|_| G.cmp(input))
                    .and_then(|_| H.cmp(input))
                    .and_then(|_| I.cmp(input))
                    .and_then(|_| J.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G, H, I, J, K> Predicate<Input>
        for And<(A, B, C, D, E, F, G, H, I, J, K)>
        where
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
            K: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J, K) = &self.0;
                A.cmp(input)
                    .and_then(|_| B.cmp(input))
                    .and_then(|_| C.cmp(input))
                    .and_then(|_| D.cmp(input))
                    .and_then(|_| E.cmp(input))
                    .and_then(|_| F.cmp(input))
                    .and_then(|_| G.cmp(input))
                    .and_then(|_| H.cmp(input))
                    .and_then(|_| I.cmp(input))
                    .and_then(|_| J.cmp(input))
                    .and_then(|_| K.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G, H, I, J, K, L> Predicate<Input>
        for And<(A, B, C, D, E, F, G, H, I, J, K, L)>
        where
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
            K: Predicate<Input>,
            L: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J, K, L) = &self.0;
                A.cmp(input)
                    .and_then(|_| B.cmp(input))
                    .and_then(|_| C.cmp(input))
                    .and_then(|_| D.cmp(input))
                    .and_then(|_| E.cmp(input))
                    .and_then(|_| F.cmp(input))
                    .and_then(|_| G.cmp(input))
                    .and_then(|_| H.cmp(input))
                    .and_then(|_| I.cmp(input))
                    .and_then(|_| J.cmp(input))
                    .and_then(|_| K.cmp(input))
                    .and_then(|_| L.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G, H, I, J, K, L, M> Predicate<Input>
        for And<(A, B, C, D, E, F, G, H, I, J, K, L, M)>
        where
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
            K: Predicate<Input>,
            L: Predicate<Input>,
            M: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J, K, L, M) = &self.0;
                A.cmp(input)
                    .and_then(|_| B.cmp(input))
                    .and_then(|_| C.cmp(input))
                    .and_then(|_| D.cmp(input))
                    .and_then(|_| E.cmp(input))
                    .and_then(|_| F.cmp(input))
                    .and_then(|_| G.cmp(input))
                    .and_then(|_| H.cmp(input))
                    .and_then(|_| I.cmp(input))
                    .and_then(|_| J.cmp(input))
                    .and_then(|_| K.cmp(input))
                    .and_then(|_| L.cmp(input))
                    .and_then(|_| M.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G, H, I, J, K, L, M, N> Predicate<Input>
        for And<(A, B, C, D, E, F, G, H, I, J, K, L, M, N)>
        where
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
            K: Predicate<Input>,
            L: Predicate<Input>,
            M: Predicate<Input>,
            N: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J, K, L, M, N) = &self.0;
                A.cmp(input)
                    .and_then(|_| B.cmp(input))
                    .and_then(|_| C.cmp(input))
                    .and_then(|_| D.cmp(input))
                    .and_then(|_| E.cmp(input))
                    .and_then(|_| F.cmp(input))
                    .and_then(|_| G.cmp(input))
                    .and_then(|_| H.cmp(input))
                    .and_then(|_| I.cmp(input))
                    .and_then(|_| J.cmp(input))
                    .and_then(|_| K.cmp(input))
                    .and_then(|_| L.cmp(input))
                    .and_then(|_| M.cmp(input))
                    .and_then(|_| N.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G, H, I, J, K, L, M, N, O> Predicate<Input>
        for And<(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O)>
        where
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
            K: Predicate<Input>,
            L: Predicate<Input>,
            M: Predicate<Input>,
            N: Predicate<Input>,
            O: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J, K, L, M, N, O) = &self.0;
                A.cmp(input)
                    .and_then(|_| B.cmp(input))
                    .and_then(|_| C.cmp(input))
                    .and_then(|_| D.cmp(input))
                    .and_then(|_| E.cmp(input))
                    .and_then(|_| F.cmp(input))
                    .and_then(|_| G.cmp(input))
                    .and_then(|_| H.cmp(input))
                    .and_then(|_| I.cmp(input))
                    .and_then(|_| J.cmp(input))
                    .and_then(|_| K.cmp(input))
                    .and_then(|_| L.cmp(input))
                    .and_then(|_| M.cmp(input))
                    .and_then(|_| N.cmp(input))
                    .and_then(|_| O.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P> Predicate<Input>
        for And<(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P)>
        where
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
            K: Predicate<Input>,
            L: Predicate<Input>,
            M: Predicate<Input>,
            N: Predicate<Input>,
            O: Predicate<Input>,
            P: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P) = &self.0;
                A.cmp(input)
                    .and_then(|_| B.cmp(input))
                    .and_then(|_| C.cmp(input))
                    .and_then(|_| D.cmp(input))
                    .and_then(|_| E.cmp(input))
                    .and_then(|_| F.cmp(input))
                    .and_then(|_| G.cmp(input))
                    .and_then(|_| H.cmp(input))
                    .and_then(|_| I.cmp(input))
                    .and_then(|_| J.cmp(input))
                    .and_then(|_| K.cmp(input))
                    .and_then(|_| L.cmp(input))
                    .and_then(|_| M.cmp(input))
                    .and_then(|_| N.cmp(input))
                    .and_then(|_| O.cmp(input))
                    .and_then(|_| P.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q> Predicate<Input>
        for And<(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q)>
        where
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
            K: Predicate<Input>,
            L: Predicate<Input>,
            M: Predicate<Input>,
            N: Predicate<Input>,
            O: Predicate<Input>,
            P: Predicate<Input>,
            Q: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q) = &self.0;
                A.cmp(input)
                    .and_then(|_| B.cmp(input))
                    .and_then(|_| C.cmp(input))
                    .and_then(|_| D.cmp(input))
                    .and_then(|_| E.cmp(input))
                    .and_then(|_| F.cmp(input))
                    .and_then(|_| G.cmp(input))
                    .and_then(|_| H.cmp(input))
                    .and_then(|_| I.cmp(input))
                    .and_then(|_| J.cmp(input))
                    .and_then(|_| K.cmp(input))
                    .and_then(|_| L.cmp(input))
                    .and_then(|_| M.cmp(input))
                    .and_then(|_| N.cmp(input))
                    .and_then(|_| O.cmp(input))
                    .and_then(|_| P.cmp(input))
                    .and_then(|_| Q.cmp(input))
            }
        }
        impl<
            Input,
            A,
            B,
            C,
            D,
            E,
            F,
            G,
            H,
            I,
            J,
            K,
            L,
            M,
            N,
            O,
            P,
            Q,
            R,
        > Predicate<Input>
        for And<(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R)>
        where
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
            K: Predicate<Input>,
            L: Predicate<Input>,
            M: Predicate<Input>,
            N: Predicate<Input>,
            O: Predicate<Input>,
            P: Predicate<Input>,
            Q: Predicate<Input>,
            R: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R) = &self.0;
                A.cmp(input)
                    .and_then(|_| B.cmp(input))
                    .and_then(|_| C.cmp(input))
                    .and_then(|_| D.cmp(input))
                    .and_then(|_| E.cmp(input))
                    .and_then(|_| F.cmp(input))
                    .and_then(|_| G.cmp(input))
                    .and_then(|_| H.cmp(input))
                    .and_then(|_| I.cmp(input))
                    .and_then(|_| J.cmp(input))
                    .and_then(|_| K.cmp(input))
                    .and_then(|_| L.cmp(input))
                    .and_then(|_| M.cmp(input))
                    .and_then(|_| N.cmp(input))
                    .and_then(|_| O.cmp(input))
                    .and_then(|_| P.cmp(input))
                    .and_then(|_| Q.cmp(input))
                    .and_then(|_| R.cmp(input))
            }
        }
        impl<
            Input,
            A,
            B,
            C,
            D,
            E,
            F,
            G,
            H,
            I,
            J,
            K,
            L,
            M,
            N,
            O,
            P,
            Q,
            R,
            S,
        > Predicate<Input>
        for And<(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S)>
        where
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
            K: Predicate<Input>,
            L: Predicate<Input>,
            M: Predicate<Input>,
            N: Predicate<Input>,
            O: Predicate<Input>,
            P: Predicate<Input>,
            Q: Predicate<Input>,
            R: Predicate<Input>,
            S: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S) = &self.0;
                A.cmp(input)
                    .and_then(|_| B.cmp(input))
                    .and_then(|_| C.cmp(input))
                    .and_then(|_| D.cmp(input))
                    .and_then(|_| E.cmp(input))
                    .and_then(|_| F.cmp(input))
                    .and_then(|_| G.cmp(input))
                    .and_then(|_| H.cmp(input))
                    .and_then(|_| I.cmp(input))
                    .and_then(|_| J.cmp(input))
                    .and_then(|_| K.cmp(input))
                    .and_then(|_| L.cmp(input))
                    .and_then(|_| M.cmp(input))
                    .and_then(|_| N.cmp(input))
                    .and_then(|_| O.cmp(input))
                    .and_then(|_| P.cmp(input))
                    .and_then(|_| Q.cmp(input))
                    .and_then(|_| R.cmp(input))
                    .and_then(|_| S.cmp(input))
            }
        }
        impl<
            Input,
            A,
            B,
            C,
            D,
            E,
            F,
            G,
            H,
            I,
            J,
            K,
            L,
            M,
            N,
            O,
            P,
            Q,
            R,
            S,
            T,
        > Predicate<Input>
        for And<(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T)>
        where
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
            K: Predicate<Input>,
            L: Predicate<Input>,
            M: Predicate<Input>,
            N: Predicate<Input>,
            O: Predicate<Input>,
            P: Predicate<Input>,
            Q: Predicate<Input>,
            R: Predicate<Input>,
            S: Predicate<Input>,
            T: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T) = &self
                    .0;
                A.cmp(input)
                    .and_then(|_| B.cmp(input))
                    .and_then(|_| C.cmp(input))
                    .and_then(|_| D.cmp(input))
                    .and_then(|_| E.cmp(input))
                    .and_then(|_| F.cmp(input))
                    .and_then(|_| G.cmp(input))
                    .and_then(|_| H.cmp(input))
                    .and_then(|_| I.cmp(input))
                    .and_then(|_| J.cmp(input))
                    .and_then(|_| K.cmp(input))
                    .and_then(|_| L.cmp(input))
                    .and_then(|_| M.cmp(input))
                    .and_then(|_| N.cmp(input))
                    .and_then(|_| O.cmp(input))
                    .and_then(|_| P.cmp(input))
                    .and_then(|_| Q.cmp(input))
                    .and_then(|_| R.cmp(input))
                    .and_then(|_| S.cmp(input))
                    .and_then(|_| T.cmp(input))
            }
        }
        impl<Input, A, B> Predicate<Input> for Or<(A, B)>
        where
            Input: ?Sized,
            A: Predicate<Input>,
            B: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B) = &self.0;
                A.cmp(input).or_else(|_| B.cmp(input))
            }
        }
        impl<Input, A, B, C> Predicate<Input> for Or<(A, B, C)>
        where
            Input: ?Sized,
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C) = &self.0;
                A.cmp(input).or_else(|_| B.cmp(input)).or_else(|_| C.cmp(input))
            }
        }
        impl<Input, A, B, C, D> Predicate<Input> for Or<(A, B, C, D)>
        where
            Input: ?Sized,
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D) = &self.0;
                A.cmp(input)
                    .or_else(|_| B.cmp(input))
                    .or_else(|_| C.cmp(input))
                    .or_else(|_| D.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E> Predicate<Input> for Or<(A, B, C, D, E)>
        where
            Input: ?Sized,
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E) = &self.0;
                A.cmp(input)
                    .or_else(|_| B.cmp(input))
                    .or_else(|_| C.cmp(input))
                    .or_else(|_| D.cmp(input))
                    .or_else(|_| E.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F> Predicate<Input> for Or<(A, B, C, D, E, F)>
        where
            Input: ?Sized,
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F) = &self.0;
                A.cmp(input)
                    .or_else(|_| B.cmp(input))
                    .or_else(|_| C.cmp(input))
                    .or_else(|_| D.cmp(input))
                    .or_else(|_| E.cmp(input))
                    .or_else(|_| F.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G> Predicate<Input> for Or<(A, B, C, D, E, F, G)>
        where
            Input: ?Sized,
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G) = &self.0;
                A.cmp(input)
                    .or_else(|_| B.cmp(input))
                    .or_else(|_| C.cmp(input))
                    .or_else(|_| D.cmp(input))
                    .or_else(|_| E.cmp(input))
                    .or_else(|_| F.cmp(input))
                    .or_else(|_| G.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G, H> Predicate<Input>
        for Or<(A, B, C, D, E, F, G, H)>
        where
            Input: ?Sized,
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H) = &self.0;
                A.cmp(input)
                    .or_else(|_| B.cmp(input))
                    .or_else(|_| C.cmp(input))
                    .or_else(|_| D.cmp(input))
                    .or_else(|_| E.cmp(input))
                    .or_else(|_| F.cmp(input))
                    .or_else(|_| G.cmp(input))
                    .or_else(|_| H.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G, H, I> Predicate<Input>
        for Or<(A, B, C, D, E, F, G, H, I)>
        where
            Input: ?Sized,
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I) = &self.0;
                A.cmp(input)
                    .or_else(|_| B.cmp(input))
                    .or_else(|_| C.cmp(input))
                    .or_else(|_| D.cmp(input))
                    .or_else(|_| E.cmp(input))
                    .or_else(|_| F.cmp(input))
                    .or_else(|_| G.cmp(input))
                    .or_else(|_| H.cmp(input))
                    .or_else(|_| I.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G, H, I, J> Predicate<Input>
        for Or<(A, B, C, D, E, F, G, H, I, J)>
        where
            Input: ?Sized,
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J) = &self.0;
                A.cmp(input)
                    .or_else(|_| B.cmp(input))
                    .or_else(|_| C.cmp(input))
                    .or_else(|_| D.cmp(input))
                    .or_else(|_| E.cmp(input))
                    .or_else(|_| F.cmp(input))
                    .or_else(|_| G.cmp(input))
                    .or_else(|_| H.cmp(input))
                    .or_else(|_| I.cmp(input))
                    .or_else(|_| J.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G, H, I, J, K> Predicate<Input>
        for Or<(A, B, C, D, E, F, G, H, I, J, K)>
        where
            Input: ?Sized,
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
            K: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J, K) = &self.0;
                A.cmp(input)
                    .or_else(|_| B.cmp(input))
                    .or_else(|_| C.cmp(input))
                    .or_else(|_| D.cmp(input))
                    .or_else(|_| E.cmp(input))
                    .or_else(|_| F.cmp(input))
                    .or_else(|_| G.cmp(input))
                    .or_else(|_| H.cmp(input))
                    .or_else(|_| I.cmp(input))
                    .or_else(|_| J.cmp(input))
                    .or_else(|_| K.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G, H, I, J, K, L> Predicate<Input>
        for Or<(A, B, C, D, E, F, G, H, I, J, K, L)>
        where
            Input: ?Sized,
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
            K: Predicate<Input>,
            L: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J, K, L) = &self.0;
                A.cmp(input)
                    .or_else(|_| B.cmp(input))
                    .or_else(|_| C.cmp(input))
                    .or_else(|_| D.cmp(input))
                    .or_else(|_| E.cmp(input))
                    .or_else(|_| F.cmp(input))
                    .or_else(|_| G.cmp(input))
                    .or_else(|_| H.cmp(input))
                    .or_else(|_| I.cmp(input))
                    .or_else(|_| J.cmp(input))
                    .or_else(|_| K.cmp(input))
                    .or_else(|_| L.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G, H, I, J, K, L, M> Predicate<Input>
        for Or<(A, B, C, D, E, F, G, H, I, J, K, L, M)>
        where
            Input: ?Sized,
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
            K: Predicate<Input>,
            L: Predicate<Input>,
            M: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J, K, L, M) = &self.0;
                A.cmp(input)
                    .or_else(|_| B.cmp(input))
                    .or_else(|_| C.cmp(input))
                    .or_else(|_| D.cmp(input))
                    .or_else(|_| E.cmp(input))
                    .or_else(|_| F.cmp(input))
                    .or_else(|_| G.cmp(input))
                    .or_else(|_| H.cmp(input))
                    .or_else(|_| I.cmp(input))
                    .or_else(|_| J.cmp(input))
                    .or_else(|_| K.cmp(input))
                    .or_else(|_| L.cmp(input))
                    .or_else(|_| M.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G, H, I, J, K, L, M, N> Predicate<Input>
        for Or<(A, B, C, D, E, F, G, H, I, J, K, L, M, N)>
        where
            Input: ?Sized,
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
            K: Predicate<Input>,
            L: Predicate<Input>,
            M: Predicate<Input>,
            N: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J, K, L, M, N) = &self.0;
                A.cmp(input)
                    .or_else(|_| B.cmp(input))
                    .or_else(|_| C.cmp(input))
                    .or_else(|_| D.cmp(input))
                    .or_else(|_| E.cmp(input))
                    .or_else(|_| F.cmp(input))
                    .or_else(|_| G.cmp(input))
                    .or_else(|_| H.cmp(input))
                    .or_else(|_| I.cmp(input))
                    .or_else(|_| J.cmp(input))
                    .or_else(|_| K.cmp(input))
                    .or_else(|_| L.cmp(input))
                    .or_else(|_| M.cmp(input))
                    .or_else(|_| N.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G, H, I, J, K, L, M, N, O> Predicate<Input>
        for Or<(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O)>
        where
            Input: ?Sized,
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
            K: Predicate<Input>,
            L: Predicate<Input>,
            M: Predicate<Input>,
            N: Predicate<Input>,
            O: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J, K, L, M, N, O) = &self.0;
                A.cmp(input)
                    .or_else(|_| B.cmp(input))
                    .or_else(|_| C.cmp(input))
                    .or_else(|_| D.cmp(input))
                    .or_else(|_| E.cmp(input))
                    .or_else(|_| F.cmp(input))
                    .or_else(|_| G.cmp(input))
                    .or_else(|_| H.cmp(input))
                    .or_else(|_| I.cmp(input))
                    .or_else(|_| J.cmp(input))
                    .or_else(|_| K.cmp(input))
                    .or_else(|_| L.cmp(input))
                    .or_else(|_| M.cmp(input))
                    .or_else(|_| N.cmp(input))
                    .or_else(|_| O.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P> Predicate<Input>
        for Or<(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P)>
        where
            Input: ?Sized,
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
            K: Predicate<Input>,
            L: Predicate<Input>,
            M: Predicate<Input>,
            N: Predicate<Input>,
            O: Predicate<Input>,
            P: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P) = &self.0;
                A.cmp(input)
                    .or_else(|_| B.cmp(input))
                    .or_else(|_| C.cmp(input))
                    .or_else(|_| D.cmp(input))
                    .or_else(|_| E.cmp(input))
                    .or_else(|_| F.cmp(input))
                    .or_else(|_| G.cmp(input))
                    .or_else(|_| H.cmp(input))
                    .or_else(|_| I.cmp(input))
                    .or_else(|_| J.cmp(input))
                    .or_else(|_| K.cmp(input))
                    .or_else(|_| L.cmp(input))
                    .or_else(|_| M.cmp(input))
                    .or_else(|_| N.cmp(input))
                    .or_else(|_| O.cmp(input))
                    .or_else(|_| P.cmp(input))
            }
        }
        impl<Input, A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q> Predicate<Input>
        for Or<(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q)>
        where
            Input: ?Sized,
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
            K: Predicate<Input>,
            L: Predicate<Input>,
            M: Predicate<Input>,
            N: Predicate<Input>,
            O: Predicate<Input>,
            P: Predicate<Input>,
            Q: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q) = &self.0;
                A.cmp(input)
                    .or_else(|_| B.cmp(input))
                    .or_else(|_| C.cmp(input))
                    .or_else(|_| D.cmp(input))
                    .or_else(|_| E.cmp(input))
                    .or_else(|_| F.cmp(input))
                    .or_else(|_| G.cmp(input))
                    .or_else(|_| H.cmp(input))
                    .or_else(|_| I.cmp(input))
                    .or_else(|_| J.cmp(input))
                    .or_else(|_| K.cmp(input))
                    .or_else(|_| L.cmp(input))
                    .or_else(|_| M.cmp(input))
                    .or_else(|_| N.cmp(input))
                    .or_else(|_| O.cmp(input))
                    .or_else(|_| P.cmp(input))
                    .or_else(|_| Q.cmp(input))
            }
        }
        impl<
            Input,
            A,
            B,
            C,
            D,
            E,
            F,
            G,
            H,
            I,
            J,
            K,
            L,
            M,
            N,
            O,
            P,
            Q,
            R,
        > Predicate<Input> for Or<(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R)>
        where
            Input: ?Sized,
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
            K: Predicate<Input>,
            L: Predicate<Input>,
            M: Predicate<Input>,
            N: Predicate<Input>,
            O: Predicate<Input>,
            P: Predicate<Input>,
            Q: Predicate<Input>,
            R: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R) = &self.0;
                A.cmp(input)
                    .or_else(|_| B.cmp(input))
                    .or_else(|_| C.cmp(input))
                    .or_else(|_| D.cmp(input))
                    .or_else(|_| E.cmp(input))
                    .or_else(|_| F.cmp(input))
                    .or_else(|_| G.cmp(input))
                    .or_else(|_| H.cmp(input))
                    .or_else(|_| I.cmp(input))
                    .or_else(|_| J.cmp(input))
                    .or_else(|_| K.cmp(input))
                    .or_else(|_| L.cmp(input))
                    .or_else(|_| M.cmp(input))
                    .or_else(|_| N.cmp(input))
                    .or_else(|_| O.cmp(input))
                    .or_else(|_| P.cmp(input))
                    .or_else(|_| Q.cmp(input))
                    .or_else(|_| R.cmp(input))
            }
        }
        impl<
            Input,
            A,
            B,
            C,
            D,
            E,
            F,
            G,
            H,
            I,
            J,
            K,
            L,
            M,
            N,
            O,
            P,
            Q,
            R,
            S,
        > Predicate<Input>
        for Or<(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S)>
        where
            Input: ?Sized,
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
            K: Predicate<Input>,
            L: Predicate<Input>,
            M: Predicate<Input>,
            N: Predicate<Input>,
            O: Predicate<Input>,
            P: Predicate<Input>,
            Q: Predicate<Input>,
            R: Predicate<Input>,
            S: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S) = &self.0;
                A.cmp(input)
                    .or_else(|_| B.cmp(input))
                    .or_else(|_| C.cmp(input))
                    .or_else(|_| D.cmp(input))
                    .or_else(|_| E.cmp(input))
                    .or_else(|_| F.cmp(input))
                    .or_else(|_| G.cmp(input))
                    .or_else(|_| H.cmp(input))
                    .or_else(|_| I.cmp(input))
                    .or_else(|_| J.cmp(input))
                    .or_else(|_| K.cmp(input))
                    .or_else(|_| L.cmp(input))
                    .or_else(|_| M.cmp(input))
                    .or_else(|_| N.cmp(input))
                    .or_else(|_| O.cmp(input))
                    .or_else(|_| P.cmp(input))
                    .or_else(|_| Q.cmp(input))
                    .or_else(|_| R.cmp(input))
                    .or_else(|_| S.cmp(input))
            }
        }
        impl<
            Input,
            A,
            B,
            C,
            D,
            E,
            F,
            G,
            H,
            I,
            J,
            K,
            L,
            M,
            N,
            O,
            P,
            Q,
            R,
            S,
            T,
        > Predicate<Input>
        for Or<(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T)>
        where
            Input: ?Sized,
            A: Predicate<Input>,
            B: Predicate<Input>,
            C: Predicate<Input>,
            D: Predicate<Input>,
            E: Predicate<Input>,
            F: Predicate<Input>,
            G: Predicate<Input>,
            H: Predicate<Input>,
            I: Predicate<Input>,
            J: Predicate<Input>,
            K: Predicate<Input>,
            L: Predicate<Input>,
            M: Predicate<Input>,
            N: Predicate<Input>,
            O: Predicate<Input>,
            P: Predicate<Input>,
            Q: Predicate<Input>,
            R: Predicate<Input>,
            S: Predicate<Input>,
            T: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let (A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T) = &self
                    .0;
                A.cmp(input)
                    .or_else(|_| B.cmp(input))
                    .or_else(|_| C.cmp(input))
                    .or_else(|_| D.cmp(input))
                    .or_else(|_| E.cmp(input))
                    .or_else(|_| F.cmp(input))
                    .or_else(|_| G.cmp(input))
                    .or_else(|_| H.cmp(input))
                    .or_else(|_| I.cmp(input))
                    .or_else(|_| J.cmp(input))
                    .or_else(|_| K.cmp(input))
                    .or_else(|_| L.cmp(input))
                    .or_else(|_| M.cmp(input))
                    .or_else(|_| N.cmp(input))
                    .or_else(|_| O.cmp(input))
                    .or_else(|_| P.cmp(input))
                    .or_else(|_| Q.cmp(input))
                    .or_else(|_| R.cmp(input))
                    .or_else(|_| S.cmp(input))
                    .or_else(|_| T.cmp(input))
            }
        }
        impl<Input, T> Predicate<Input> for Not<T>
        where
            T: Predicate<Input>,
        {
            fn cmp(&self, value: &Input) -> Result<(), ErrorKind> {
                if self.0.cmp(value).is_err() { Ok(()) } else { Err(ErrorKind::Not) }
            }
        }
        impl<Input, T, U> Predicate<Input> for When<T, U>
        where
            T: Predicate<Input>,
            U: Predicate<Input>,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                if self.condition.cmp(input).is_ok() {
                    self.predicate.cmp(input)
                } else {
                    Ok(())
                }
            }
        }
        impl<Input, T> Predicate<Input> for T
        where
            T: Fn(&Input) -> Result<(), ErrorKind> + Copy,
        {
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                self(input)
            }
        }
    }
    pub use error::ErrorKind;
    pub use header::header;
    pub use method::{is_mutation, is_safe};
    pub use predicate::*;
    use crate::request::Request;
    use crate::{BoxFuture, Error, Middleware, Next};
    /// Stop processing the request and respond if the provided precondition fails.
    ///
    /// Guard wraps a synchronous check function `T` that receives a reference to
    /// the incoming request and returns a result.
    ///
    /// This is useful for lightweight, synchronous validations that do not require
    /// async work—such as authorization checks based on headers, request metadata,
    /// or other inexpensive predicates.
    ///
    /// # Example
    ///
    /// ```rust
    /// use via::{Request, guard, raise};
    ///
    /// let mut app = via::app(());
    /// let mut api = app.route("/api");
    ///
    /// api.middleware(via::guard(
    ///     || raise!(401, message = "\"x-api-key\" is missing or invalid."),
    ///     |request: &Request<_>| {
    ///         request.headers().get("x-api-key").is_some_and(|value| {
    ///             todo!("validate api key value");
    ///         })
    ///     },
    /// ));
    ///
    /// // Subsequent routes have a valid API key.
    ///
    /// api.route("/users").scope(|users| {
    ///     // Define the /api/users resource.
    /// });
    /// ```
    pub struct Guard<E, T> {
        or_else: E,
        predicate: T,
    }
    /// Confirm that request matches the provided predicate before proceeding.
    ///
    /// # Example
    ///
    /// ```rust
    /// use via::{Request, raise};
    ///
    /// let mut app = via::app(());
    /// let mut api = app.route("/api");
    ///
    /// api.middleware(via::guard(
    ///     || raise!(401, message = "\"x-api-key\" is missing or invalid."),
    ///     |request: &Request<_>| {
    ///         request.headers().get("x-api-key").is_some_and(|value| {
    ///             todo!("validate api key value");
    ///         })
    ///     },
    /// ));
    ///
    /// // Subsequent routes have a valid API key.
    ///
    /// api.route("/users").scope(|users| {
    ///     // Define the /api/users resource.
    /// });
    /// ```
    ///
    pub fn guard<E, T>(or_else: E, predicate: T) -> Guard<E, T> {
        Guard { or_else, predicate }
    }
    impl<E, T, App> Middleware<App> for Guard<E, T>
    where
        E: Fn(ErrorKind) -> Error + Copy + Send + Sync,
        T: Predicate<Request<App>> + Send + Sync,
    {
        fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture {
            match self.predicate.cmp(&request) {
                Ok(_) => next.call(request),
                Err(kind) => {
                    let error = (self.or_else)(kind);
                    Box::pin(async { Err(error) })
                }
            }
        }
    }
}
pub mod request {
    pub mod params {
        use std::borrow::Cow;
        use std::str::FromStr;
        use super::query::{QueryParamRange, QueryParser};
        use crate::error::{Error, ResultExt};
        use crate::util::UriEncoding;
        pub struct PathParam<'a, 'b> {
            encoding: UriEncoding,
            source: &'a str,
            param: Option<&'a via_router::PathParam>,
            name: &'b str,
        }
        pub struct PathParams<'a> {
            path: &'a str,
            spans: &'a [via_router::PathParam],
        }
        #[automatically_derived]
        #[doc(hidden)]
        unsafe impl<'a> ::core::clone::TrivialClone for PathParams<'a> {}
        #[automatically_derived]
        impl<'a> ::core::clone::Clone for PathParams<'a> {
            #[inline]
            fn clone(&self) -> PathParams<'a> {
                let _: ::core::clone::AssertParamIsClone<&'a str>;
                let _: ::core::clone::AssertParamIsClone<&'a [via_router::PathParam]>;
                *self
            }
        }
        #[automatically_derived]
        impl<'a> ::core::marker::Copy for PathParams<'a> {}
        pub struct QueryParam<'a, 'b> {
            encoding: UriEncoding,
            source: Option<&'a str>,
            range: Option<QueryParamRange>,
            name: &'b str,
        }
        pub struct QueryParams<'a> {
            query: Option<&'a str>,
            spans: Vec<(Cow<'a, str>, Option<QueryParamRange>)>,
        }
        pub(crate) fn get<'a>(
            spans: &'a [via_router::PathParam],
            name: &str,
        ) -> Option<&'a via_router::PathParam> {
            spans.iter().find(|param| name == param.ident())
        }
        fn query_pos_for_key(
            predicate: &str,
            key: &str,
            value: &Option<[Option<usize>; 2]>,
        ) -> Option<[Option<usize>; 2]> {
            if key == predicate { value.as_ref().copied() } else { None }
        }
        impl<'a> PathParams<'a> {
            pub fn get<'b>(&self, name: &'b str) -> PathParam<'a, 'b> {
                PathParam::new(self.path, get(self.spans, name), name)
            }
        }
        impl<'a> PathParams<'a> {
            pub(crate) fn new(
                path: &'a str,
                spans: &'a [via_router::PathParam],
            ) -> Self {
                Self { path, spans }
            }
        }
        impl<'a> QueryParams<'a> {
            pub(crate) fn new(query: Option<&'a str>) -> Self {
                let spans = query
                    .map(|input| QueryParser::new(input).collect())
                    .unwrap_or_default();
                Self { query, spans }
            }
            pub fn all<'b>(
                &self,
                name: &'b str,
            ) -> impl Iterator<Item = QueryParam<'a, 'b>> {
                self.spans
                    .iter()
                    .filter_map(move |(key, value)| {
                        let value = value.as_ref();
                        if key.as_ref() == name {
                            Some(QueryParam::new(self.query, value.copied(), name))
                        } else {
                            None
                        }
                    })
            }
            pub fn contains(&self, name: &str) -> bool {
                self.spans.iter().any(|(key, _)| key.as_ref() == name)
            }
            pub fn first<'b>(&self, name: &'b str) -> QueryParam<'a, 'b> {
                let range = self
                    .spans
                    .iter()
                    .find_map(|(key, value)| query_pos_for_key(name, key, value));
                QueryParam::new(self.query, range, name)
            }
            pub fn last<'b>(&self, name: &'b str) -> QueryParam<'a, 'b> {
                let range = self
                    .spans
                    .iter()
                    .rev()
                    .find_map(|(key, value)| query_pos_for_key(name, key, value));
                QueryParam::new(self.query, range, name)
            }
        }
        impl<'a, 'b> PathParam<'a, 'b> {
            #[inline]
            pub(crate) fn new(
                source: &'a str,
                param: Option<&'a via_router::PathParam>,
                name: &'b str,
            ) -> Self {
                Self {
                    encoding: UriEncoding::Unencoded,
                    source,
                    param,
                    name,
                }
            }
            /// Returns a new `Param` that will percent-decode the parameter value with
            /// when the parameter is converted to a result.
            ///
            #[inline]
            pub fn percent_decode(self) -> Self {
                Self {
                    encoding: UriEncoding::Percent,
                    ..self
                }
            }
            /// Calls [`str::parse`] on the parameter value if it exists and returns the
            /// result. If the param is encoded, it will be decoded before it is parsed.
            ///
            pub fn parse<U>(self) -> Result<U, Error>
            where
                U: FromStr,
                Error: From<U::Err>,
            {
                self.into_result()
                    .and_then(|value| value.as_ref().parse().or_bad_request())
            }
            pub fn ok(self) -> Result<Option<Cow<'a, str>>, Error> {
                self.param
                    .and_then(|param| param.slice(self.source))
                    .map(|value| self.encoding.decode_as(self.name, value))
                    .transpose()
            }
        }
        impl<'a, 'b> ResultExt for PathParam<'a, 'b> {
            type Output = Cow<'a, str>;
            /// Returns a result with the parameter value if it exists.
            #[inline]
            fn into_result(self) -> Result<Self::Output, Error> {
                self.param
                    .and_then(|param| param.slice(self.source))
                    .ok_or_else(|| Error::require_path_param(self.name))
                    .and_then(|value| self.encoding.decode_as(self.name, value))
            }
        }
        impl<'a, 'b> QueryParam<'a, 'b> {
            #[inline]
            pub(crate) fn new(
                source: Option<&'a str>,
                range: Option<[Option<usize>; 2]>,
                name: &'b str,
            ) -> Self {
                Self {
                    encoding: UriEncoding::Unencoded,
                    source,
                    range,
                    name,
                }
            }
            /// Returns a new `Param` that will percent-decode the parameter value with
            /// when the parameter is converted to a result.
            ///
            #[inline]
            pub fn percent_decode(self) -> Self {
                Self {
                    encoding: UriEncoding::Percent,
                    ..self
                }
            }
            /// Calls [`str::parse`] on the parameter value if it exists and returns the
            /// result. If the param is encoded, it will be decoded before it is parsed.
            ///
            pub fn parse<U>(self) -> Result<U, Error>
            where
                U: FromStr,
                Error: From<U::Err>,
            {
                self.into_result()
                    .and_then(|value| value.as_ref().parse().or_bad_request())
            }
            pub fn ok(self) -> Result<Option<Cow<'a, str>>, Error> {
                self.slice()
                    .map(|value| self.encoding.decode_as(self.name, value))
                    .transpose()
            }
        }
        impl<'a, 'b> QueryParam<'a, 'b> {
            /// Returns a new `Param` that will percent-decode the parameter value with
            /// when the parameter is converted to a result.
            ///
            #[inline]
            fn slice(&self) -> Option<&'a str> {
                self.source
                    .zip(self.range)
                    .and_then(|(source, span)| match span {
                        [Some(from), Some(to)] if from == to => None,
                        [Some(from), Some(to)] => source.get(from..to),
                        [Some(from), None] => source.get(from..),
                        [None, _] => None,
                    })
            }
        }
        impl<'a, 'b> ResultExt for QueryParam<'a, 'b> {
            type Output = Cow<'a, str>;
            /// Returns a result with the parameter value if it exists.
            #[inline]
            fn into_result(self) -> Result<Self::Output, Error> {
                self.slice()
                    .ok_or_else(|| Error::require_query_param(self.name))
                    .and_then(|value| self.encoding.decode_as(self.name, value))
            }
        }
    }
    mod payload {
        use bytes::{Buf, Bytes};
        use http::HeaderMap;
        use http_body::{Body, Frame, SizeHint};
        use hyper::body::Incoming;
        use serde::de::DeserializeOwned;
        use std::future::Future;
        use std::marker::PhantomData;
        use std::pin::Pin;
        use std::ptr;
        use std::rc::Rc;
        use std::sync::atomic::{Ordering, compiler_fence};
        use std::task::{Context, Poll, ready};
        use crate::error::Error;
        mod sealed {
            /// Prevents external implementations of Payload. Allowing us to make
            /// assumptions about the data contained by implementations of Payload.
            pub trait Sealed {}
            impl Sealed for super::Aggregate {}
            impl Sealed for bytes::Bytes {}
            impl Sealed for tungstenite::protocol::Message {}
        }
        /// Represents an optionally contiguous source of data received from a client.
        ///
        /// The methods defined in the `Payload` trait also provide counterparts with
        /// zeroization guarantees, ensuring that the original buffers are securely
        /// cleared after the data is read.
        ///
        /// # Memory Hygiene
        ///
        /// Payload methods take ownership of `self` to prevent accidental reuse of
        /// volatile buffers. This behavior ensures that once the data is coalesced or
        /// deserialized, the original memory is unreachable.
        ///
        /// ## Zeroization
        ///
        /// The majority of use-cases where zeroization is preferred but not strictly
        /// necessary can benefit from using the `bez_*` prefixed versions of the
        /// methods defined in the `Payload` trait. The `be_z` prefix stands for
        /// "best-effort zeroization". If zeroization is impossible due to non-unique
        /// access of a buffer contained in the payload, `bez_*` variations fall back
        /// to their non-zeroing counterparts.
        ///
        /// If zeroization is a hard requirement, we recommend defining a policy that
        /// is sufficient for your business use-case. For example, returning an opaque
        /// 500 error to the client and immediately stopping request processing is likely
        /// enough to satisfy the definition of "fair handling of user data". As always,
        /// we suggest defining a policy and working with compliance and legal to
        /// determine what is right for your situation.
        ///
        /// In any case, users should avoid retaining the payload returned in the `Err`
        /// branch of strict zeroizing methods prefixed by `z_*` and stop processing
        /// the request as soon as possible. This reduces the likelihood of a panic
        /// crashing a connection task, potentially (albeit unlikely) exposing
        /// un-zeroed memory.
        ///
        pub trait Payload: sealed::Sealed + Sized {
            /// Coalesces all non-contiguous bytes into a single contiguous `Vec<u8>`.
            ///
            fn coalesce(self) -> Vec<u8>;
            /// Coalesces all non-contiguous bytes into a single contiguous `Vec<u8>`.
            ///
            /// If zeroization is impossible due to non-unique access of an underlying
            /// frame buffer, `self` is returned to the caller.
            ///
            /// # Security
            ///
            /// Users should avoid retaining the returned `Self` in `Err` longer than
            /// necessary, as it contains un-zeroed memory.
            ///
            fn z_coalesce(self) -> Result<Vec<u8>, Self>;
            /// Deserialize the payload as JSON into the specified type `T`.
            ///
            /// # Errors
            ///
            /// - `Err(Error)` if `T` cannot be deserialized from the data in `self`
            ///
            fn json<T>(self) -> Result<T, Error>
            where
                T: DeserializeOwned;
            /// Deserialize the payload as JSON into the specified type `T`, zeroizing
            /// the original data from which the `T` is deserialized.
            ///
            /// # Errors
            ///
            /// - `Err(Self)` if zeroization is impossible due to non-unique access
            /// - `Ok(Err(Error))` if `T` cannot be deserialized from the data in `self`
            ///
            /// # Security
            ///
            /// Users should avoid retaining the returned `Self` in `Err` longer than
            /// necessary, as it contains un-zeroed memory.
            ///
            fn z_json<T>(self) -> Result<Result<T, Error>, Self>
            where
                T: DeserializeOwned,
            {
                self.z_coalesce().map(|data| deserialize_json(data.as_slice()))
            }
            /// Deserialize the payload as JSON into the specified type `T`, zeroizing
            /// the original data from which the `T` is deserialized.
            ///
            /// If zeroization is impossible due to non-unique access, fallback to
            /// [`Payload::json`].
            ///
            /// # Errors
            ///
            /// - `Err(Error)` if `T` cannot be deserialized from the data in `self`
            ///
            fn bez_json<T>(self) -> Result<T, Error>
            where
                T: DeserializeOwned,
            {
                self.z_json().unwrap_or_else(Self::json)
            }
            /// Converts the payload into a UTF-8 `String`.
            ///
            /// # Errors
            ///
            /// - `Err(Error)` if the payload contains an invalid UTF-8 byte sequence
            ///
            fn utf8(self) -> Result<String, Error> {
                deserialize_utf8(self.coalesce())
            }
            /// Converts the payload into a UTF-8 `String`, zeroizing the original data
            /// from which the `String` is constructed.
            ///
            /// # Errors
            ///
            /// - `Err(Self)` if zeroization is impossible due to non-unique access
            /// - `Ok(Err(Error))` if the payload contains an invalid UTF-8 byte
            ///   sequence
            ///
            fn z_utf8(self) -> Result<Result<String, Error>, Self> {
                self.z_coalesce().map(deserialize_utf8)
            }
            /// Converts the payload into a UTF-8 `String`, zeroizing the original data
            /// from which the `String` is constructed.
            ///
            /// If zeroization is impossible due to non-unique access, fallback to
            /// [`Payload::utf8`].
            ///
            /// # Errors
            ///
            /// - `Err(Error)` if the payload contains an invalid UTF-8 byte sequence
            ///
            fn bez_utf8(self) -> Result<String, Error> {
                self.z_utf8().unwrap_or_else(Self::utf8)
            }
        }
        /// The data and trailers of a request body.
        ///
        pub struct Aggregate {
            payload: RequestPayload,
            _unsend: PhantomData<Rc<()>>,
        }
        #[must_use = "futures do nothing unless you `.await` or poll them"]
        pub struct Coalesce {
            body: RequestBody,
            trailers: Option<HeaderMap>,
        }
        pub struct RequestBody {
            remaining: usize,
            body: Incoming,
            frames: Option<Vec<Bytes>>,
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for RequestBody {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::debug_struct_field3_finish(
                    f,
                    "RequestBody",
                    "remaining",
                    &self.remaining,
                    "body",
                    &self.body,
                    "frames",
                    &&self.frames,
                )
            }
        }
        struct RequestPayload {
            frames: Vec<Bytes>,
            trailers: Option<HeaderMap>,
        }
        #[inline]
        fn deserialize_json<T>(buf: &[u8]) -> Result<T, Error>
        where
            T: DeserializeOwned,
        {
            serde_json::from_slice(buf).map_err(Error::de_json)
        }
        #[inline]
        fn deserialize_utf8(data: Vec<u8>) -> Result<String, Error> {
            String::from_utf8(data)
                .map_err(|_| Error::invalid_utf8_sequence("request body"))
        }
        /// Zeroize the buffer backing the provided `Bytes`. Afterwards, the data in
        /// `frame` is unreachable and the visible length is 0.
        ///
        /// Adapted from the [zeroize] crate in order to prevent an O(n) call to
        /// compiler_fence where n is the number of frames in a payload.
        ///
        /// To safely call this fn, you must guarantee the following invariants:
        ///
        ///   1. `Bytes::is_unique` is true for `frame`
        ///   2. `compiler_fence` is called after each frame is zeroized
        ///
        /// [zeroize]: https://crates.io/crates/zeroize/
        #[inline(never)]
        unsafe fn unfenced_zeroize(frame: &mut Bytes) {
            let len = frame.remaining();
            let ptr = frame.as_ptr() as *mut u8;
            for idx in 0..len {
                unsafe {
                    ptr::write_volatile(ptr.add(idx), 0);
                }
            }
            frame.advance(len);
        }
        #[inline(always)]
        fn release_compiler_fence() {
            compiler_fence(Ordering::Release);
        }
        impl Aggregate {
            pub fn trailers(&self) -> Option<&HeaderMap> {
                self.payload.trailers.as_ref()
            }
            pub fn is_empty(&self) -> bool {
                self.len().is_some_and(|len| len == 0)
            }
            #[inline]
            pub fn len(&self) -> Option<usize> {
                self.payload
                    .frames()
                    .iter()
                    .map(Buf::remaining)
                    .try_fold(0usize, |len, remaining| len.checked_add(remaining))
            }
        }
        impl Aggregate {
            fn new(payload: RequestPayload) -> Self {
                Self {
                    payload,
                    _unsend: PhantomData,
                }
            }
        }
        impl Payload for Aggregate {
            fn coalesce(mut self) -> Vec<u8> {
                let mut dest = self.len().map(Vec::with_capacity).unwrap_or_default();
                for frame in self.payload.frames_mut().iter_mut() {
                    dest.extend_from_slice(frame.as_ref());
                    frame.advance(frame.remaining());
                }
                dest
            }
            fn json<T>(mut self) -> Result<T, Error>
            where
                T: DeserializeOwned,
            {
                if let [frame] = self.payload.frames_mut() {
                    let result = deserialize_json(frame.as_ref());
                    frame.advance(frame.remaining());
                    return result;
                }
                deserialize_json(self.coalesce().as_slice())
            }
            fn z_json<T>(mut self) -> Result<Result<T, Error>, Self>
            where
                T: DeserializeOwned,
            {
                if let [frame] = self.payload.frames_mut() {
                    if !frame.is_unique() {
                        return Err(self);
                    }
                    let result = deserialize_json(frame.as_ref());
                    unsafe {
                        unfenced_zeroize(frame);
                    }
                    release_compiler_fence();
                    return Ok(result);
                }
                self.z_coalesce().map(|data| deserialize_json(data.as_slice()))
            }
            fn z_coalesce(mut self) -> Result<Vec<u8>, Self> {
                let mut dest = self.len().map(Vec::with_capacity).unwrap_or_default();
                let payload = &mut self.payload;
                if !payload.frames().iter().all(Bytes::is_unique) {
                    return Err(self);
                }
                for frame in payload.frames_mut().iter_mut() {
                    dest.extend_from_slice(frame.as_ref());
                    unsafe {
                        unfenced_zeroize(frame);
                    }
                }
                release_compiler_fence();
                Ok(dest)
            }
        }
        impl Payload for tungstenite::protocol::Message {
            fn coalesce(self) -> Vec<u8> {
                Payload::coalesce(Bytes::from((|this| this)(self)))
            }
            fn z_coalesce(self) -> Result<Vec<u8>, Self> {
                Payload::z_coalesce(Bytes::from((|this| this)(self))).map_err(From::from)
            }
            fn json<T>(self) -> Result<T, Error>
            where
                T: DeserializeOwned,
            {
                Payload::json(Bytes::from((|this| this)(self)))
            }
            fn z_json<T>(self) -> Result<Result<T, Error>, Self>
            where
                T: DeserializeOwned,
            {
                Payload::z_json(Bytes::from((|this| this)(self))).map_err(From::from)
            }
            fn z_utf8(self) -> Result<Result<String, Error>, Self> {
                Payload::z_utf8(Bytes::from((|this| this)(self))).map_err(From::from)
            }
        }
        impl Payload for Bytes {
            fn coalesce(mut self) -> Vec<u8> {
                let mut dest = Vec::with_capacity(self.remaining());
                dest.extend_from_slice(self.as_ref());
                self.advance(self.remaining());
                dest
            }
            fn z_coalesce(mut self) -> Result<Vec<u8>, Self> {
                if !self.is_unique() {
                    return Err(self);
                }
                let mut dest = Vec::with_capacity(self.remaining());
                dest.extend_from_slice(self.as_ref());
                unsafe {
                    unfenced_zeroize(&mut self);
                }
                release_compiler_fence();
                Ok(dest)
            }
            fn json<T>(mut self) -> Result<T, Error>
            where
                T: DeserializeOwned,
            {
                let result = deserialize_json(self.as_ref());
                self.advance(self.remaining());
                result
            }
            fn z_json<T>(mut self) -> Result<Result<T, Error>, Self>
            where
                T: DeserializeOwned,
            {
                if !self.is_unique() {
                    return Err(self);
                }
                let result = deserialize_json(self.as_ref());
                unsafe {
                    unfenced_zeroize(&mut self);
                }
                release_compiler_fence();
                Ok(result)
            }
        }
        impl Coalesce {
            pub(super) fn new(body: RequestBody) -> Self {
                Self { body, trailers: None }
            }
        }
        fn already_read() -> Error {
            Error::new("a request body can only be read once.")
        }
        fn unknown_frame_type() -> Error {
            Error::new("unknown frame type received while reading a request body.")
        }
        impl Future for Coalesce {
            type Output = Result<Aggregate, Error>;
            fn poll(
                mut self: Pin<&mut Self>,
                context: &mut Context,
            ) -> Poll<Self::Output> {
                while let Some(frame) = match Pin::new(&mut self.body)
                    .poll_frame(context)?
                {
                    ::core::task::Poll::Ready(t) => t,
                    ::core::task::Poll::Pending => {
                        return ::core::task::Poll::Pending;
                    }
                } {
                    match frame.into_data() {
                        Ok(data) => {
                            self.body.frames_mut()?.push(data);
                        }
                        Err(frame) => {
                            let trailers = frame
                                .into_trailers()
                                .map_err(|_| unknown_frame_type())?;
                            if let Some(existing) = self.trailers.as_mut() {
                                existing.extend(trailers);
                            } else {
                                self.trailers = Some(trailers);
                            }
                        }
                    }
                }
                Poll::Ready(
                    Ok(
                        Aggregate::new(RequestPayload {
                            frames: self.body.end()?,
                            trailers: self.trailers.take(),
                        }),
                    ),
                )
            }
        }
        impl RequestBody {
            pub(crate) fn new(
                remaining: usize,
                body: Incoming,
                frames: Vec<Bytes>,
            ) -> Self {
                Self {
                    remaining,
                    body,
                    frames: Some(frames),
                }
            }
            fn has_capacity(&self) -> bool {
                self.body
                    .size_hint()
                    .exact()
                    .is_none_or(|upper| {
                        u64::try_from(self.remaining)
                            .is_ok_and(|remaining| remaining >= upper)
                    })
            }
            fn frames_mut(&mut self) -> Result<&mut Vec<Bytes>, Error> {
                self.frames.as_mut().ok_or_else(already_read)
            }
            fn end(&mut self) -> Result<Vec<Bytes>, Error> {
                self.frames.take().ok_or_else(already_read)
            }
        }
        impl Body for RequestBody {
            type Data = Bytes;
            type Error = Error;
            fn poll_frame(
                mut self: Pin<&mut Self>,
                context: &mut Context,
            ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
                if self.remaining == 0 || !self.has_capacity() {
                    return Poll::Ready(Some(Err(Error::payload_too_large())));
                }
                let Some(frame) = (match Pin::new(&mut self.body).poll_frame(context)? {
                    ::core::task::Poll::Ready(t) => t,
                    ::core::task::Poll::Pending => {
                        return ::core::task::Poll::Pending;
                    }
                }) else {
                    return Poll::Ready(None);
                };
                if let Some(data) = frame.data_ref() {
                    self.remaining = self
                        .remaining
                        .checked_sub(data.remaining())
                        .ok_or_else(|| {
                            self.remaining = 0;
                            Error::payload_too_large()
                        })?;
                }
                Poll::Ready(Some(Ok(frame)))
            }
            fn is_end_stream(&self) -> bool {
                self.remaining == 0 || !self.has_capacity() || self.body.is_end_stream()
            }
            fn size_hint(&self) -> SizeHint {
                let Ok(remaining) = u64::try_from(self.remaining) else {
                    let mut hint = SizeHint::new();
                    hint.set_lower(self.body.size_hint().lower());
                    if true {
                        use std::sync::Once;
                        static ONCE: Once = Once::new();
                        ONCE.call_once(|| {
                            {
                                ::std::io::_print(
                                    format_args!(
                                        "warn: a lossy size hint must be used for RequestBody. ",
                                    ),
                                );
                            };
                            {
                                ::std::io::_print(
                                    format_args!(
                                        "usize::MAX exceeds u64::MAX on this platform.\n",
                                    ),
                                );
                            };
                        });
                    }
                    return hint;
                };
                let mut hint = self.body.size_hint();
                if remaining < hint.lower() {
                    hint.set_exact(remaining);
                } else {
                    let upper = hint
                        .upper()
                        .map_or(remaining, |upper| upper.min(remaining));
                    hint.set_upper(upper);
                }
                hint
            }
        }
        impl RequestPayload {
            #[inline]
            fn frames(&self) -> &[Bytes] {
                &self.frames
            }
            #[inline]
            fn frames_mut(&mut self) -> &mut [Bytes] {
                &mut self.frames
            }
        }
    }
    mod query {
        use percent_encoding::percent_decode_str;
        use std::borrow::Cow;
        pub type QueryParamRange = [Option<usize>; 2];
        pub struct QueryParser<'a> {
            input: &'a str,
            from: usize,
        }
        fn decode(input: &str) -> Cow<'_, str> {
            percent_decode_str(input).decode_utf8_lossy()
        }
        fn take_name(input: &str, from: usize) -> (usize, Option<Cow<'_, str>>) {
            let len = input.len();
            let at = take_while(input, from, |byte| byte == b'&')
                .map(|start| {
                    match take_while(input, start, |byte| byte != b'=') {
                        Some(end) => (start, end),
                        None => (start, len),
                    }
                });
            match at {
                Some((start, end)) => (end, input.get(start..end).map(decode)),
                None => (len, None),
            }
        }
        fn take_value(input: &str, from: usize) -> (usize, Option<QueryParamRange>) {
            let len = input.len();
            let at = take_while(input, from, |byte| byte == b'=')
                .map(|start| {
                    match take_while(input, start, |byte| byte != b'&') {
                        Some(end) => [Some(start), Some(end)],
                        None => [Some(start), None],
                    }
                });
            match at {
                Some([_, Some(end)]) => (end, at),
                Some([_, None]) => (len, at),
                None => (len, None),
            }
        }
        fn take_while(
            input: &str,
            from: usize,
            f: impl Fn(u8) -> bool,
        ) -> Option<usize> {
            let rest = input.get(from..)?;
            rest.bytes()
                .enumerate()
                .find_map(|(to, byte)| {
                    if !f(byte) { from.checked_add(to) } else { None }
                })
        }
        impl<'a> QueryParser<'a> {
            pub fn new(input: &'a str) -> Self {
                Self { input, from: 0 }
            }
        }
        impl<'a> Iterator for QueryParser<'a> {
            type Item = (Cow<'a, str>, Option<QueryParamRange>);
            fn next(&mut self) -> Option<Self::Item> {
                let (start, name) = take_name(self.input, self.from);
                let (end, at) = take_value(self.input, start);
                self.from = end;
                name.zip(Some(at))
            }
        }
    }
    pub use params::{PathParams, QueryParams};
    pub use payload::{Aggregate, Coalesce, Payload, RequestBody};
    use cookie::CookieJar;
    use delegate::delegate;
    use http::request::Parts;
    use http::{Extensions, HeaderMap, Method, Uri, Version};
    use std::fmt::{self, Debug, Formatter};
    use crate::ResultExt;
    use crate::app::Shared;
    use crate::error::Error;
    use crate::response::{Finalize, Response, ResponseBuilder};
    use params::PathParam;
    pub struct Envelope {
        parts: Parts,
        params: Vec<via_router::PathParam>,
        cookies: CookieJar,
    }
    pub struct Request<App = ()> {
        envelope: Envelope,
        body: RequestBody,
        app: Shared<App>,
    }
    impl Envelope {
        /// Returns a reference to the request's method.
        ///
        #[inline]
        pub fn method(&self) -> &Method {
            &self.parts.method
        }
        /// Returns a reference to the request's URI.
        ///
        #[inline]
        pub fn uri(&self) -> &Uri {
            &self.parts.uri
        }
        /// Returns the HTTP version that was used to make the request.
        ///
        #[inline]
        pub fn version(&self) -> Version {
            self.parts.version
        }
        /// Returns a reference to the request's headers.
        ///
        #[inline]
        pub fn headers(&self) -> &HeaderMap {
            &self.parts.headers
        }
        /// Returns reference to the cookies associated with the request.
        ///
        #[inline]
        pub fn cookies(&self) -> &CookieJar {
            &self.cookies
        }
        /// Returns a mutable reference to the cookies associated with the request.
        ///
        #[inline]
        pub fn cookies_mut(&mut self) -> &mut CookieJar {
            &mut self.cookies
        }
        /// Returns a reference to the associated extensions.
        ///
        #[inline]
        pub fn extensions(&self) -> &Extensions {
            &self.parts.extensions
        }
        /// Returns a mutable reference to the associated extensions.
        ///
        #[inline]
        pub fn extensions_mut(&mut self) -> &mut Extensions {
            &mut self.parts.extensions
        }
        /// Returns a convenient wrapper around an optional reference to the path
        /// parameter in the request's uri with the provided `name`.
        ///
        pub fn param<'b>(&self, name: &'b str) -> PathParam<'_, 'b> {
            let param = params::get(&self.params, name);
            PathParam::new(self.uri().path(), param, name)
        }
        pub fn query<'a, T>(&'a self) -> crate::Result<T>
        where
            T: TryFrom<QueryParams<'a>, Error = Error>,
        {
            T::try_from(QueryParams::new(self.uri().query()))
        }
        pub fn params<'a, T>(&'a self) -> crate::Result<T>
        where
            T: TryFrom<PathParams<'a>>,
            Error: From<T::Error>,
        {
            let path = self.uri().path();
            let params = &self.params;
            T::try_from(PathParams::new(path, params)).or_bad_request()
        }
    }
    impl Envelope {
        #[inline]
        pub(crate) fn new(parts: Parts, params: Vec<via_router::PathParam>) -> Self {
            Self {
                parts,
                params,
                cookies: CookieJar::new(),
            }
        }
    }
    impl Debug for Envelope {
        fn fmt(&self, f: &mut Formatter) -> fmt::Result {
            struct CookieJar;
            #[automatically_derived]
            impl ::core::fmt::Debug for CookieJar {
                #[inline]
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::write_str(f, "CookieJar")
                }
            }
            f.debug_struct("Envelope")
                .field("method", self.method())
                .field("uri", self.uri())
                .field("params", &self.params)
                .field("version", &self.version())
                .field("headers", self.headers())
                .field("cookies", &CookieJar)
                .field("extensions", self.extensions())
                .finish()
        }
    }
    impl<App> Request<App> {
        #[inline]
        pub(crate) fn new(
            envelope: Envelope,
            body: RequestBody,
            app: Shared<App>,
        ) -> Self {
            Self { envelope, body, app }
        }
        #[inline]
        pub fn app(&self) -> &App {
            &self.app
        }
        pub fn app_owned(&self) -> Shared<App> {
            self.app.clone()
        }
        #[inline]
        pub fn envelope(&self) -> &Envelope {
            &self.envelope
        }
        /// Returns a reference to the request's method.
        #[inline]
        pub fn method(&self) -> &Method {
            self.envelope().method()
        }
        /// Returns a reference to the request's URI.
        #[inline]
        pub fn uri(&self) -> &Uri {
            self.envelope().uri()
        }
        /// Returns the HTTP version that was used to make the request.
        #[inline]
        pub fn version(&self) -> Version {
            self.envelope().version()
        }
        /// Returns a reference to the request's headers.
        #[inline]
        pub fn headers(&self) -> &HeaderMap {
            self.envelope().headers()
        }
        /// Returns reference to the cookies associated with the request.
        #[inline]
        pub fn cookies(&self) -> &CookieJar {
            self.envelope().cookies()
        }
        /// Returns a mutable reference to the cookies associated with the request.
        pub fn cookies_mut(&mut self) -> &mut CookieJar {
            self.envelope.cookies_mut()
        }
        /// Returns a reference to the associated extensions.
        pub fn extensions(&self) -> &Extensions {
            self.envelope().extensions()
        }
        /// Returns a mutable reference to the associated extensions.
        #[inline]
        pub fn extensions_mut(&mut self) -> &mut Extensions {
            self.envelope.extensions_mut()
        }
        /// Returns reference to the cookies associated with the request.
        #[inline]
        pub fn param<'b>(&self, name: &'b str) -> PathParam<'_, 'b> {
            self.envelope().param(name)
        }
        #[inline]
        pub fn query<'a, T>(&'a self) -> crate::Result<T>
        where
            T: TryFrom<QueryParams<'a>, Error = Error>,
        {
            self.envelope().query::<T>()
        }
        #[inline]
        pub fn params<'a, T>(&'a self) -> crate::Result<T>
        where
            T: TryFrom<PathParams<'a>>,
            Error: From<T::Error>,
        {
            self.envelope().params::<T>()
        }
        /// Consumes the request and returns a tuple containing a future that
        /// resolves with the data and trailers of the body as well as a shared
        /// copy of `App`.
        ///
        pub fn into_future(self) -> (Coalesce, Shared<App>) {
            (Coalesce::new(self.body), self.app)
        }
        /// Consumes the request and returns a tuple containing it's parts.
        ///
        #[inline]
        pub fn into_parts(self) -> (Envelope, RequestBody, Shared<App>) {
            (self.envelope, self.body, self.app)
        }
    }
    impl<App> Debug for Request<App> {
        fn fmt(&self, f: &mut Formatter) -> fmt::Result {
            f.debug_struct("Request")
                .field("envelope", self.envelope())
                .field("body", &self.body)
                .field("app", &self.app)
                .finish()
        }
    }
    impl<App> Finalize for Request<App> {
        fn finalize(self, response: ResponseBuilder) -> Result<Response, Error> {
            use http::header::{CONTENT_LENGTH, CONTENT_TYPE, TRANSFER_ENCODING};
            use http_body_util::combinators::BoxBody;
            let headers = self.headers();
            let mut response = match headers.get(CONTENT_LENGTH).cloned() {
                Some(content_length) => response.header(CONTENT_LENGTH, content_length),
                None => response.header(TRANSFER_ENCODING, "chunked"),
            };
            if let Some(content_type) = headers.get(CONTENT_TYPE).cloned() {
                response = response.header(CONTENT_TYPE, content_type);
            }
            response.body(BoxBody::new(self.body).into())
        }
    }
}
pub mod response {
    mod body {
        use bytes::Bytes;
        use http_body::{Body, Frame, SizeHint};
        use http_body_util::combinators::BoxBody;
        use http_body_util::{BodyExt, Full};
        use std::fmt::{self, Debug, Formatter};
        use std::pin::Pin;
        use std::task::{Context, Poll};
        use crate::Error;
        pub struct ResponseBody {
            body: BoxBody<Bytes, Error>,
        }
        impl ResponseBody {
            #[inline]
            pub fn new(buf: Bytes) -> Self {
                Self::boxed(Full::new(buf).map_err(|_| Error::new("unreachable")))
            }
            #[inline]
            pub fn boxed<T>(body: T) -> Self
            where
                T: Body<Data = Bytes, Error = Error> + Send + Sync + 'static,
            {
                Self { body: BoxBody::new(body) }
            }
        }
        impl Body for ResponseBody {
            type Data = Bytes;
            type Error = Error;
            fn poll_frame(
                mut self: Pin<&mut Self>,
                context: &mut Context,
            ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
                Pin::new(&mut self.body).poll_frame(context)
            }
            fn is_end_stream(&self) -> bool {
                self.body.is_end_stream()
            }
            fn size_hint(&self) -> SizeHint {
                self.body.size_hint()
            }
        }
        impl Debug for ResponseBody {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.debug_struct("ResponseBody").finish()
            }
        }
        impl Default for ResponseBody {
            #[inline]
            fn default() -> Self {
                Self::new(Default::default())
            }
        }
        impl From<BoxBody<Bytes, Error>> for ResponseBody {
            #[inline]
            fn from(body: BoxBody<Bytes, Error>) -> Self {
                Self { body }
            }
        }
        impl From<Bytes> for ResponseBody {
            #[inline]
            fn from(buf: Bytes) -> Self {
                Self::new(buf)
            }
        }
        impl From<String> for ResponseBody {
            #[inline]
            fn from(data: String) -> Self {
                Self::new(Bytes::from(data.into_bytes()))
            }
        }
        impl From<&'_ str> for ResponseBody {
            #[inline]
            fn from(data: &str) -> Self {
                Self::new(Bytes::copy_from_slice(data.as_bytes()))
            }
        }
        impl From<Vec<u8>> for ResponseBody {
            #[inline]
            fn from(data: Vec<u8>) -> Self {
                Self::new(Bytes::from(data))
            }
        }
        impl From<&'_ [u8]> for ResponseBody {
            #[inline]
            fn from(slice: &'_ [u8]) -> Self {
                Self::new(Bytes::copy_from_slice(slice))
            }
        }
    }
    mod builder {
        use bytes::Bytes;
        use futures_core::Stream;
        use http::header::{CONTENT_LENGTH, CONTENT_TYPE, TRANSFER_ENCODING};
        use http::{HeaderName, HeaderValue, StatusCode, Version};
        use http_body::Frame;
        use http_body_util::StreamBody;
        use serde::Serialize;
        use super::Response;
        use super::body::ResponseBody;
        use crate::error::Error;
        /// Define how a type finalizes a [`ResponseBuilder`].
        ///
        /// ```
        /// use via::response::{Finalize, Response};
        /// use via::{Next, Request};
        ///
        /// async fn echo(request: Request, _: Next) -> via::Result {
        ///     request.finalize(Response::build().header("X-Powered-By", "Via"))
        /// }
        /// ```
        ///
        pub trait Finalize {
            fn finalize(self, response: ResponseBuilder) -> Result<Response, Error>;
        }
        pub struct ResponseBuilder {
            response: http::response::Builder,
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for ResponseBuilder {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::debug_struct_field1_finish(
                    f,
                    "ResponseBuilder",
                    "response",
                    &&self.response,
                )
            }
        }
        #[automatically_derived]
        impl ::core::default::Default for ResponseBuilder {
            #[inline]
            fn default() -> ResponseBuilder {
                ResponseBuilder {
                    response: ::core::default::Default::default(),
                }
            }
        }
        impl ResponseBuilder {
            #[inline]
            pub fn status<T>(mut self, status: T) -> Self
            where
                StatusCode: TryFrom<T>,
                <StatusCode as TryFrom<T>>::Error: Into<http::Error>,
            {
                self.response = self.response.status(status);
                self
            }
            #[inline]
            pub fn version(mut self, version: Version) -> Self {
                self.response = self.response.version(version);
                self
            }
            #[inline]
            pub fn header<K, V>(mut self, key: K, value: V) -> Self
            where
                HeaderName: TryFrom<K>,
                <HeaderName as TryFrom<K>>::Error: Into<http::Error>,
                HeaderValue: TryFrom<V>,
                <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
            {
                self.response = self.response.header(key, value);
                self
            }
            #[inline]
            pub fn extension<T>(mut self, extension: T) -> Self
            where
                T: Clone + Send + Sync + 'static,
            {
                self.response = self.response.extension(extension);
                self
            }
            #[inline]
            pub fn body(self, body: ResponseBody) -> Result<Response, Error> {
                Ok(self.response.body(body)?.into())
            }
            #[inline]
            pub fn json(self, body: &impl Serialize) -> Result<Response, Error> {
                let body = serde_json::to_vec(body).map_err(Error::ser_json)?;
                self.header(CONTENT_LENGTH, body.len())
                    .header(CONTENT_TYPE, "application/json; charset=utf-8")
                    .body(ResponseBody::from(body))
            }
            #[inline]
            pub fn html(self, body: impl Into<String>) -> Result<Response, Error> {
                let body = body.into();
                self.header(CONTENT_LENGTH, body.len())
                    .header(CONTENT_TYPE, "text/html; charset=utf-8")
                    .body(ResponseBody::from(body))
            }
            #[inline]
            pub fn text(self, body: impl Into<String>) -> Result<Response, Error> {
                let body = body.into();
                self.header(CONTENT_LENGTH, body.len())
                    .header(CONTENT_TYPE, "text/plain; charset=utf-8")
                    .body(ResponseBody::from(body))
            }
            /// Convert self into a [Response] with an empty payload.
            ///
            #[inline]
            pub fn finish(self) -> Result<Response, Error> {
                self.body(ResponseBody::default())
            }
        }
        impl<T> Finalize for T
        where
            T: Stream<Item = Result<Frame<Bytes>, Error>> + Send + Sync + 'static,
        {
            #[inline]
            fn finalize(self, builder: ResponseBuilder) -> Result<Response, Error> {
                builder
                    .header(TRANSFER_ENCODING, "chunked")
                    .body(ResponseBody::boxed(StreamBody::new(self)))
            }
        }
    }
    mod redirect {
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
            pub fn with_status(
                location: &str,
                status: StatusCode,
            ) -> Result<Response, Error> {
                if !status.is_redirection() {
                    if true {
                        {
                            ::std::io::_eprint(
                                format_args!(
                                    "error: redirect status out of range {0}\n",
                                    status,
                                ),
                            );
                        };
                    }
                    {
                        let status = crate::error::StatusCode::INTERNAL_SERVER_ERROR;
                        let message = status
                            .canonical_reason()
                            .unwrap_or_default()
                            .to_owned()
                            .to_ascii_lowercase() + ".";
                        return Err(crate::error::deny(status, message));
                    };
                }
                Response::build().status(status).header(LOCATION, location).finish()
            }
        }
    }
    pub use body::ResponseBody;
    pub use builder::{Finalize, ResponseBuilder};
    use delegate::delegate;
    pub use redirect::Redirect;
    use cookie::CookieJar;
    use http::{Extensions, HeaderMap, StatusCode, Version};
    use std::fmt::{self, Debug, Formatter};
    pub struct Response {
        inner: http::Response<ResponseBody>,
        cookies: CookieJar,
    }
    impl Response {
        #[inline]
        pub fn new(body: ResponseBody) -> Self {
            Self {
                inner: http::Response::new(body),
                cookies: CookieJar::new(),
            }
        }
        #[inline]
        pub fn build() -> ResponseBuilder {
            Default::default()
        }
        #[inline]
        pub fn status(&self) -> StatusCode {
            self.inner().status()
        }
        #[inline]
        pub fn status_mut(&mut self) -> &mut StatusCode {
            self.inner_mut().status_mut()
        }
        #[inline]
        pub fn version(&self) -> Version {
            self.inner().version()
        }
        #[inline]
        pub fn headers(&self) -> &HeaderMap {
            self.inner().headers()
        }
        #[inline]
        pub fn headers_mut(&mut self) -> &mut HeaderMap {
            self.inner_mut().headers_mut()
        }
        /// Returns a reference to the response cookies.
        pub fn cookies(&self) -> &CookieJar {
            &self.cookies
        }
        /// Returns a mutable reference to the response cookies.
        pub fn cookies_mut(&mut self) -> &mut CookieJar {
            &mut self.cookies
        }
        #[inline]
        pub fn extensions(&self) -> &Extensions {
            self.inner().extensions()
        }
        #[inline]
        pub fn extensions_mut(&mut self) -> &mut Extensions {
            self.inner_mut().extensions_mut()
        }
        #[inline]
        fn inner(&self) -> &http::Response<ResponseBody> {
            &self.inner
        }
        #[inline]
        fn inner_mut(&mut self) -> &mut http::Response<ResponseBody> {
            &mut self.inner
        }
    }
    impl Debug for Response {
        fn fmt(&self, f: &mut Formatter) -> fmt::Result {
            f.debug_struct("Response")
                .field("status", &self.status())
                .field("version", &self.version())
                .field("headers", self.headers())
                .field("cookies", &self.cookies)
                .field("body", self.inner.body())
                .finish()
        }
    }
    impl From<Response> for http::Response<ResponseBody> {
        #[inline]
        fn from(response: Response) -> Self {
            response.inner
        }
    }
    impl From<http::Response<ResponseBody>> for Response {
        #[inline]
        fn from(inner: http::Response<ResponseBody>) -> Self {
            Self {
                inner,
                cookies: CookieJar::new(),
            }
        }
    }
}
pub mod router {
    mod allow {
        use bitflags::bitflags;
        use std::fmt::{self, Display, Formatter};
        use crate::middleware::{BoxFuture, Middleware};
        use crate::next::{Continue, Next};
        use crate::{Error, Request};
        pub struct Allow<T> {
            middleware: T,
            mask: Mask,
        }
        pub struct Branch<T, U> {
            middleware: T,
            or_else: U,
            mask: Mask,
        }
        /// Stop processing the request and respond with `405` Method Not Allowed.
        ///
        pub struct Deny {
            allow: Mask,
        }
        pub(crate) struct MethodNotAllowed {
            allow: Mask,
            method: Mask,
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for MethodNotAllowed {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::debug_struct_field2_finish(
                    f,
                    "MethodNotAllowed",
                    "allow",
                    &self.allow,
                    "method",
                    &&self.method,
                )
            }
        }
        trait Predicate {
            fn matches(&self, other: &Mask) -> bool;
        }
        struct Mask(<Mask as ::bitflags::__private::PublicFlags>::Internal);
        #[automatically_derived]
        #[doc(hidden)]
        unsafe impl ::core::clone::TrivialClone for Mask {}
        #[automatically_derived]
        impl ::core::clone::Clone for Mask {
            #[inline]
            fn clone(&self) -> Mask {
                let _: ::core::clone::AssertParamIsClone<
                    <Mask as ::bitflags::__private::PublicFlags>::Internal,
                >;
                *self
            }
        }
        #[automatically_derived]
        impl ::core::marker::Copy for Mask {}
        #[automatically_derived]
        impl ::core::fmt::Debug for Mask {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::debug_tuple_field1_finish(f, "Mask", &&self.0)
            }
        }
        #[automatically_derived]
        impl ::core::cmp::Eq for Mask {
            #[inline]
            #[doc(hidden)]
            #[coverage(off)]
            fn assert_receiver_is_total_eq(&self) -> () {
                let _: ::core::cmp::AssertParamIsEq<
                    <Mask as ::bitflags::__private::PublicFlags>::Internal,
                >;
            }
        }
        #[automatically_derived]
        impl ::core::marker::StructuralPartialEq for Mask {}
        #[automatically_derived]
        impl ::core::cmp::PartialEq for Mask {
            #[inline]
            fn eq(&self, other: &Mask) -> bool {
                self.0 == other.0
            }
        }
        impl Mask {
            #[allow(deprecated, non_upper_case_globals)]
            pub const CONNECT: Self = Self::from_bits_retain(1 << 0);
            #[allow(deprecated, non_upper_case_globals)]
            pub const DELETE: Self = Self::from_bits_retain(1 << 1);
            #[allow(deprecated, non_upper_case_globals)]
            pub const GET: Self = Self::from_bits_retain(1 << 2);
            #[allow(deprecated, non_upper_case_globals)]
            pub const HEAD: Self = Self::from_bits_retain(1 << 3);
            #[allow(deprecated, non_upper_case_globals)]
            pub const OPTIONS: Self = Self::from_bits_retain(1 << 4);
            #[allow(deprecated, non_upper_case_globals)]
            pub const PATCH: Self = Self::from_bits_retain(1 << 5);
            #[allow(deprecated, non_upper_case_globals)]
            pub const POST: Self = Self::from_bits_retain(1 << 6);
            #[allow(deprecated, non_upper_case_globals)]
            pub const PUT: Self = Self::from_bits_retain(1 << 7);
            #[allow(deprecated, non_upper_case_globals)]
            pub const TRACE: Self = Self::from_bits_retain(1 << 8);
        }
        impl ::bitflags::Flags for Mask {
            const FLAGS: &'static [::bitflags::Flag<Mask>] = &[
                {
                    #[allow(deprecated, non_upper_case_globals)]
                    ::bitflags::Flag::new("CONNECT", Mask::CONNECT)
                },
                {
                    #[allow(deprecated, non_upper_case_globals)]
                    ::bitflags::Flag::new("DELETE", Mask::DELETE)
                },
                {
                    #[allow(deprecated, non_upper_case_globals)]
                    ::bitflags::Flag::new("GET", Mask::GET)
                },
                {
                    #[allow(deprecated, non_upper_case_globals)]
                    ::bitflags::Flag::new("HEAD", Mask::HEAD)
                },
                {
                    #[allow(deprecated, non_upper_case_globals)]
                    ::bitflags::Flag::new("OPTIONS", Mask::OPTIONS)
                },
                {
                    #[allow(deprecated, non_upper_case_globals)]
                    ::bitflags::Flag::new("PATCH", Mask::PATCH)
                },
                {
                    #[allow(deprecated, non_upper_case_globals)]
                    ::bitflags::Flag::new("POST", Mask::POST)
                },
                {
                    #[allow(deprecated, non_upper_case_globals)]
                    ::bitflags::Flag::new("PUT", Mask::PUT)
                },
                {
                    #[allow(deprecated, non_upper_case_globals)]
                    ::bitflags::Flag::new("TRACE", Mask::TRACE)
                },
            ];
            type Bits = u16;
            fn bits(&self) -> u16 {
                Mask::bits(self)
            }
            fn from_bits_retain(bits: u16) -> Mask {
                Mask::from_bits_retain(bits)
            }
        }
        #[allow(
            dead_code,
            deprecated,
            unused_doc_comments,
            unused_attributes,
            unused_mut,
            unused_imports,
            non_upper_case_globals,
            clippy::assign_op_pattern,
            clippy::indexing_slicing,
            clippy::same_name_method,
            clippy::iter_without_into_iter,
        )]
        const _: () = {
            #[repr(transparent)]
            struct InternalBitFlags(u16);
            #[automatically_derived]
            #[doc(hidden)]
            unsafe impl ::core::clone::TrivialClone for InternalBitFlags {}
            #[automatically_derived]
            impl ::core::clone::Clone for InternalBitFlags {
                #[inline]
                fn clone(&self) -> InternalBitFlags {
                    let _: ::core::clone::AssertParamIsClone<u16>;
                    *self
                }
            }
            #[automatically_derived]
            impl ::core::marker::Copy for InternalBitFlags {}
            #[automatically_derived]
            impl ::core::marker::StructuralPartialEq for InternalBitFlags {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for InternalBitFlags {
                #[inline]
                fn eq(&self, other: &InternalBitFlags) -> bool {
                    self.0 == other.0
                }
            }
            #[automatically_derived]
            impl ::core::cmp::Eq for InternalBitFlags {
                #[inline]
                #[doc(hidden)]
                #[coverage(off)]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<u16>;
                }
            }
            #[automatically_derived]
            impl ::core::cmp::PartialOrd for InternalBitFlags {
                #[inline]
                fn partial_cmp(
                    &self,
                    other: &InternalBitFlags,
                ) -> ::core::option::Option<::core::cmp::Ordering> {
                    ::core::cmp::PartialOrd::partial_cmp(&self.0, &other.0)
                }
            }
            #[automatically_derived]
            impl ::core::cmp::Ord for InternalBitFlags {
                #[inline]
                fn cmp(&self, other: &InternalBitFlags) -> ::core::cmp::Ordering {
                    ::core::cmp::Ord::cmp(&self.0, &other.0)
                }
            }
            #[automatically_derived]
            impl ::core::hash::Hash for InternalBitFlags {
                #[inline]
                fn hash<__H: ::core::hash::Hasher>(&self, state: &mut __H) -> () {
                    ::core::hash::Hash::hash(&self.0, state)
                }
            }
            impl ::bitflags::__private::PublicFlags for Mask {
                type Primitive = u16;
                type Internal = InternalBitFlags;
            }
            impl ::bitflags::__private::core::default::Default for InternalBitFlags {
                #[inline]
                fn default() -> Self {
                    InternalBitFlags::empty()
                }
            }
            impl ::bitflags::__private::core::fmt::Debug for InternalBitFlags {
                fn fmt(
                    &self,
                    f: &mut ::bitflags::__private::core::fmt::Formatter<'_>,
                ) -> ::bitflags::__private::core::fmt::Result {
                    if self.is_empty() {
                        f.write_fmt(
                            format_args!("{0:#x}", <u16 as ::bitflags::Bits>::EMPTY),
                        )
                    } else {
                        ::bitflags::__private::core::fmt::Display::fmt(self, f)
                    }
                }
            }
            impl ::bitflags::__private::core::fmt::Display for InternalBitFlags {
                fn fmt(
                    &self,
                    f: &mut ::bitflags::__private::core::fmt::Formatter<'_>,
                ) -> ::bitflags::__private::core::fmt::Result {
                    ::bitflags::parser::to_writer(&Mask(*self), f)
                }
            }
            impl ::bitflags::__private::core::str::FromStr for InternalBitFlags {
                type Err = ::bitflags::parser::ParseError;
                fn from_str(
                    s: &str,
                ) -> ::bitflags::__private::core::result::Result<Self, Self::Err> {
                    ::bitflags::parser::from_str::<Mask>(s).map(|flags| flags.0)
                }
            }
            impl ::bitflags::__private::core::convert::AsRef<u16> for InternalBitFlags {
                fn as_ref(&self) -> &u16 {
                    &self.0
                }
            }
            impl ::bitflags::__private::core::convert::From<u16> for InternalBitFlags {
                fn from(bits: u16) -> Self {
                    Self::from_bits_retain(bits)
                }
            }
            #[allow(dead_code, deprecated, unused_attributes)]
            impl InternalBitFlags {
                /// Get a flags value with all bits unset.
                #[inline]
                pub const fn empty() -> Self {
                    Self(<u16 as ::bitflags::Bits>::EMPTY)
                }
                /// Get a flags value with all known bits set.
                #[inline]
                pub const fn all() -> Self {
                    let mut truncated = <u16 as ::bitflags::Bits>::EMPTY;
                    let mut i = 0;
                    {
                        {
                            let flag = <Mask as ::bitflags::Flags>::FLAGS[i]
                                .value()
                                .bits();
                            truncated = truncated | flag;
                            i += 1;
                        }
                    };
                    {
                        {
                            let flag = <Mask as ::bitflags::Flags>::FLAGS[i]
                                .value()
                                .bits();
                            truncated = truncated | flag;
                            i += 1;
                        }
                    };
                    {
                        {
                            let flag = <Mask as ::bitflags::Flags>::FLAGS[i]
                                .value()
                                .bits();
                            truncated = truncated | flag;
                            i += 1;
                        }
                    };
                    {
                        {
                            let flag = <Mask as ::bitflags::Flags>::FLAGS[i]
                                .value()
                                .bits();
                            truncated = truncated | flag;
                            i += 1;
                        }
                    };
                    {
                        {
                            let flag = <Mask as ::bitflags::Flags>::FLAGS[i]
                                .value()
                                .bits();
                            truncated = truncated | flag;
                            i += 1;
                        }
                    };
                    {
                        {
                            let flag = <Mask as ::bitflags::Flags>::FLAGS[i]
                                .value()
                                .bits();
                            truncated = truncated | flag;
                            i += 1;
                        }
                    };
                    {
                        {
                            let flag = <Mask as ::bitflags::Flags>::FLAGS[i]
                                .value()
                                .bits();
                            truncated = truncated | flag;
                            i += 1;
                        }
                    };
                    {
                        {
                            let flag = <Mask as ::bitflags::Flags>::FLAGS[i]
                                .value()
                                .bits();
                            truncated = truncated | flag;
                            i += 1;
                        }
                    };
                    {
                        {
                            let flag = <Mask as ::bitflags::Flags>::FLAGS[i]
                                .value()
                                .bits();
                            truncated = truncated | flag;
                            i += 1;
                        }
                    };
                    let _ = i;
                    Self(truncated)
                }
                /// Get the underlying bits value.
                ///
                /// The returned value is exactly the bits set in this flags value.
                #[inline]
                pub const fn bits(&self) -> u16 {
                    self.0
                }
                /// Convert from a bits value.
                ///
                /// This method will return `None` if any unknown bits are set.
                #[inline]
                pub const fn from_bits(
                    bits: u16,
                ) -> ::bitflags::__private::core::option::Option<Self> {
                    let truncated = Self::from_bits_truncate(bits).0;
                    if truncated == bits {
                        ::bitflags::__private::core::option::Option::Some(Self(bits))
                    } else {
                        ::bitflags::__private::core::option::Option::None
                    }
                }
                /// Convert from a bits value, unsetting any unknown bits.
                #[inline]
                pub const fn from_bits_truncate(bits: u16) -> Self {
                    Self(bits & Self::all().0)
                }
                /// Convert from a bits value exactly.
                #[inline]
                pub const fn from_bits_retain(bits: u16) -> Self {
                    Self(bits)
                }
                /// Get a flags value with the bits of a flag with the given name set.
                ///
                /// This method will return `None` if `name` is empty or doesn't
                /// correspond to any named flag.
                #[inline]
                pub fn from_name(
                    name: &str,
                ) -> ::bitflags::__private::core::option::Option<Self> {
                    {
                        if name == "CONNECT" {
                            return ::bitflags::__private::core::option::Option::Some(
                                Self(Mask::CONNECT.bits()),
                            );
                        }
                    };
                    {
                        if name == "DELETE" {
                            return ::bitflags::__private::core::option::Option::Some(
                                Self(Mask::DELETE.bits()),
                            );
                        }
                    };
                    {
                        if name == "GET" {
                            return ::bitflags::__private::core::option::Option::Some(
                                Self(Mask::GET.bits()),
                            );
                        }
                    };
                    {
                        if name == "HEAD" {
                            return ::bitflags::__private::core::option::Option::Some(
                                Self(Mask::HEAD.bits()),
                            );
                        }
                    };
                    {
                        if name == "OPTIONS" {
                            return ::bitflags::__private::core::option::Option::Some(
                                Self(Mask::OPTIONS.bits()),
                            );
                        }
                    };
                    {
                        if name == "PATCH" {
                            return ::bitflags::__private::core::option::Option::Some(
                                Self(Mask::PATCH.bits()),
                            );
                        }
                    };
                    {
                        if name == "POST" {
                            return ::bitflags::__private::core::option::Option::Some(
                                Self(Mask::POST.bits()),
                            );
                        }
                    };
                    {
                        if name == "PUT" {
                            return ::bitflags::__private::core::option::Option::Some(
                                Self(Mask::PUT.bits()),
                            );
                        }
                    };
                    {
                        if name == "TRACE" {
                            return ::bitflags::__private::core::option::Option::Some(
                                Self(Mask::TRACE.bits()),
                            );
                        }
                    };
                    let _ = name;
                    ::bitflags::__private::core::option::Option::None
                }
                /// Whether all bits in this flags value are unset.
                #[inline]
                pub const fn is_empty(&self) -> bool {
                    self.0 == <u16 as ::bitflags::Bits>::EMPTY
                }
                /// Whether all known bits in this flags value are set.
                #[inline]
                pub const fn is_all(&self) -> bool {
                    Self::all().0 | self.0 == self.0
                }
                /// Whether any set bits in a source flags value are also set in a target flags value.
                #[inline]
                pub const fn intersects(&self, other: Self) -> bool {
                    self.0 & other.0 != <u16 as ::bitflags::Bits>::EMPTY
                }
                /// Whether all set bits in a source flags value are also set in a target flags value.
                #[inline]
                pub const fn contains(&self, other: Self) -> bool {
                    self.0 & other.0 == other.0
                }
                /// The bitwise or (`|`) of the bits in two flags values.
                #[inline]
                pub fn insert(&mut self, other: Self) {
                    *self = Self(self.0).union(other);
                }
                /// The intersection of a source flags value with the complement of a target flags
                /// value (`&!`).
                ///
                /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
                /// `remove` won't truncate `other`, but the `!` operator will.
                #[inline]
                pub fn remove(&mut self, other: Self) {
                    *self = Self(self.0).difference(other);
                }
                /// The bitwise exclusive-or (`^`) of the bits in two flags values.
                #[inline]
                pub fn toggle(&mut self, other: Self) {
                    *self = Self(self.0).symmetric_difference(other);
                }
                /// Call `insert` when `value` is `true` or `remove` when `value` is `false`.
                #[inline]
                pub fn set(&mut self, other: Self, value: bool) {
                    if value {
                        self.insert(other);
                    } else {
                        self.remove(other);
                    }
                }
                /// The bitwise and (`&`) of the bits in two flags values.
                #[inline]
                #[must_use]
                pub const fn intersection(self, other: Self) -> Self {
                    Self(self.0 & other.0)
                }
                /// The bitwise or (`|`) of the bits in two flags values.
                #[inline]
                #[must_use]
                pub const fn union(self, other: Self) -> Self {
                    Self(self.0 | other.0)
                }
                /// The intersection of a source flags value with the complement of a target flags
                /// value (`&!`).
                ///
                /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
                /// `difference` won't truncate `other`, but the `!` operator will.
                #[inline]
                #[must_use]
                pub const fn difference(self, other: Self) -> Self {
                    Self(self.0 & !other.0)
                }
                /// The bitwise exclusive-or (`^`) of the bits in two flags values.
                #[inline]
                #[must_use]
                pub const fn symmetric_difference(self, other: Self) -> Self {
                    Self(self.0 ^ other.0)
                }
                /// The bitwise negation (`!`) of the bits in a flags value, truncating the result.
                #[inline]
                #[must_use]
                pub const fn complement(self) -> Self {
                    Self::from_bits_truncate(!self.0)
                }
            }
            impl ::bitflags::__private::core::fmt::Binary for InternalBitFlags {
                fn fmt(
                    &self,
                    f: &mut ::bitflags::__private::core::fmt::Formatter,
                ) -> ::bitflags::__private::core::fmt::Result {
                    let inner = self.0;
                    ::bitflags::__private::core::fmt::Binary::fmt(&inner, f)
                }
            }
            impl ::bitflags::__private::core::fmt::Octal for InternalBitFlags {
                fn fmt(
                    &self,
                    f: &mut ::bitflags::__private::core::fmt::Formatter,
                ) -> ::bitflags::__private::core::fmt::Result {
                    let inner = self.0;
                    ::bitflags::__private::core::fmt::Octal::fmt(&inner, f)
                }
            }
            impl ::bitflags::__private::core::fmt::LowerHex for InternalBitFlags {
                fn fmt(
                    &self,
                    f: &mut ::bitflags::__private::core::fmt::Formatter,
                ) -> ::bitflags::__private::core::fmt::Result {
                    let inner = self.0;
                    ::bitflags::__private::core::fmt::LowerHex::fmt(&inner, f)
                }
            }
            impl ::bitflags::__private::core::fmt::UpperHex for InternalBitFlags {
                fn fmt(
                    &self,
                    f: &mut ::bitflags::__private::core::fmt::Formatter,
                ) -> ::bitflags::__private::core::fmt::Result {
                    let inner = self.0;
                    ::bitflags::__private::core::fmt::UpperHex::fmt(&inner, f)
                }
            }
            impl ::bitflags::__private::core::ops::BitOr for InternalBitFlags {
                type Output = Self;
                /// The bitwise or (`|`) of the bits in two flags values.
                #[inline]
                fn bitor(self, other: InternalBitFlags) -> Self {
                    self.union(other)
                }
            }
            impl ::bitflags::__private::core::ops::BitOrAssign for InternalBitFlags {
                /// The bitwise or (`|`) of the bits in two flags values.
                #[inline]
                fn bitor_assign(&mut self, other: Self) {
                    self.insert(other);
                }
            }
            impl ::bitflags::__private::core::ops::BitXor for InternalBitFlags {
                type Output = Self;
                /// The bitwise exclusive-or (`^`) of the bits in two flags values.
                #[inline]
                fn bitxor(self, other: Self) -> Self {
                    self.symmetric_difference(other)
                }
            }
            impl ::bitflags::__private::core::ops::BitXorAssign for InternalBitFlags {
                /// The bitwise exclusive-or (`^`) of the bits in two flags values.
                #[inline]
                fn bitxor_assign(&mut self, other: Self) {
                    self.toggle(other);
                }
            }
            impl ::bitflags::__private::core::ops::BitAnd for InternalBitFlags {
                type Output = Self;
                /// The bitwise and (`&`) of the bits in two flags values.
                #[inline]
                fn bitand(self, other: Self) -> Self {
                    self.intersection(other)
                }
            }
            impl ::bitflags::__private::core::ops::BitAndAssign for InternalBitFlags {
                /// The bitwise and (`&`) of the bits in two flags values.
                #[inline]
                fn bitand_assign(&mut self, other: Self) {
                    *self = Self::from_bits_retain(self.bits()).intersection(other);
                }
            }
            impl ::bitflags::__private::core::ops::Sub for InternalBitFlags {
                type Output = Self;
                /// The intersection of a source flags value with the complement of a target flags value (`&!`).
                ///
                /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
                /// `difference` won't truncate `other`, but the `!` operator will.
                #[inline]
                fn sub(self, other: Self) -> Self {
                    self.difference(other)
                }
            }
            impl ::bitflags::__private::core::ops::SubAssign for InternalBitFlags {
                /// The intersection of a source flags value with the complement of a target flags value (`&!`).
                ///
                /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
                /// `difference` won't truncate `other`, but the `!` operator will.
                #[inline]
                fn sub_assign(&mut self, other: Self) {
                    self.remove(other);
                }
            }
            impl ::bitflags::__private::core::ops::Not for InternalBitFlags {
                type Output = Self;
                /// The bitwise negation (`!`) of the bits in a flags value, truncating the result.
                #[inline]
                fn not(self) -> Self {
                    self.complement()
                }
            }
            impl ::bitflags::__private::core::iter::Extend<InternalBitFlags>
            for InternalBitFlags {
                /// The bitwise or (`|`) of the bits in each flags value.
                fn extend<
                    T: ::bitflags::__private::core::iter::IntoIterator<Item = Self>,
                >(&mut self, iterator: T) {
                    for item in iterator {
                        self.insert(item)
                    }
                }
            }
            impl ::bitflags::__private::core::iter::FromIterator<InternalBitFlags>
            for InternalBitFlags {
                /// The bitwise or (`|`) of the bits in each flags value.
                fn from_iter<
                    T: ::bitflags::__private::core::iter::IntoIterator<Item = Self>,
                >(iterator: T) -> Self {
                    use ::bitflags::__private::core::iter::Extend;
                    let mut result = Self::empty();
                    result.extend(iterator);
                    result
                }
            }
            impl InternalBitFlags {
                /// Yield a set of contained flags values.
                ///
                /// Each yielded flags value will correspond to a defined named flag. Any unknown bits
                /// will be yielded together as a final flags value.
                #[inline]
                pub const fn iter(&self) -> ::bitflags::iter::Iter<Mask> {
                    ::bitflags::iter::Iter::__private_const_new(
                        <Mask as ::bitflags::Flags>::FLAGS,
                        Mask::from_bits_retain(self.bits()),
                        Mask::from_bits_retain(self.bits()),
                    )
                }
                /// Yield a set of contained named flags values.
                ///
                /// This method is like [`iter`](#method.iter), except only yields bits in contained named flags.
                /// Any unknown bits, or bits not corresponding to a contained flag will not be yielded.
                #[inline]
                pub const fn iter_names(&self) -> ::bitflags::iter::IterNames<Mask> {
                    ::bitflags::iter::IterNames::__private_const_new(
                        <Mask as ::bitflags::Flags>::FLAGS,
                        Mask::from_bits_retain(self.bits()),
                        Mask::from_bits_retain(self.bits()),
                    )
                }
            }
            impl ::bitflags::__private::core::iter::IntoIterator for InternalBitFlags {
                type Item = Mask;
                type IntoIter = ::bitflags::iter::Iter<Mask>;
                fn into_iter(self) -> Self::IntoIter {
                    self.iter()
                }
            }
            impl InternalBitFlags {
                /// Returns a mutable reference to the raw value of the flags currently stored.
                #[inline]
                pub fn bits_mut(&mut self) -> &mut u16 {
                    &mut self.0
                }
            }
            #[allow(dead_code, deprecated, unused_attributes)]
            impl Mask {
                /// Get a flags value with all bits unset.
                #[inline]
                pub const fn empty() -> Self {
                    Self(InternalBitFlags::empty())
                }
                /// Get a flags value with all known bits set.
                #[inline]
                pub const fn all() -> Self {
                    Self(InternalBitFlags::all())
                }
                /// Get the underlying bits value.
                ///
                /// The returned value is exactly the bits set in this flags value.
                #[inline]
                pub const fn bits(&self) -> u16 {
                    self.0.bits()
                }
                /// Convert from a bits value.
                ///
                /// This method will return `None` if any unknown bits are set.
                #[inline]
                pub const fn from_bits(
                    bits: u16,
                ) -> ::bitflags::__private::core::option::Option<Self> {
                    match InternalBitFlags::from_bits(bits) {
                        ::bitflags::__private::core::option::Option::Some(bits) => {
                            ::bitflags::__private::core::option::Option::Some(Self(bits))
                        }
                        ::bitflags::__private::core::option::Option::None => {
                            ::bitflags::__private::core::option::Option::None
                        }
                    }
                }
                /// Convert from a bits value, unsetting any unknown bits.
                #[inline]
                pub const fn from_bits_truncate(bits: u16) -> Self {
                    Self(InternalBitFlags::from_bits_truncate(bits))
                }
                /// Convert from a bits value exactly.
                #[inline]
                pub const fn from_bits_retain(bits: u16) -> Self {
                    Self(InternalBitFlags::from_bits_retain(bits))
                }
                /// Get a flags value with the bits of a flag with the given name set.
                ///
                /// This method will return `None` if `name` is empty or doesn't
                /// correspond to any named flag.
                #[inline]
                pub fn from_name(
                    name: &str,
                ) -> ::bitflags::__private::core::option::Option<Self> {
                    match InternalBitFlags::from_name(name) {
                        ::bitflags::__private::core::option::Option::Some(bits) => {
                            ::bitflags::__private::core::option::Option::Some(Self(bits))
                        }
                        ::bitflags::__private::core::option::Option::None => {
                            ::bitflags::__private::core::option::Option::None
                        }
                    }
                }
                /// Whether all bits in this flags value are unset.
                #[inline]
                pub const fn is_empty(&self) -> bool {
                    self.0.is_empty()
                }
                /// Whether all known bits in this flags value are set.
                #[inline]
                pub const fn is_all(&self) -> bool {
                    self.0.is_all()
                }
                /// Whether any set bits in a source flags value are also set in a target flags value.
                #[inline]
                pub const fn intersects(&self, other: Self) -> bool {
                    self.0.intersects(other.0)
                }
                /// Whether all set bits in a source flags value are also set in a target flags value.
                #[inline]
                pub const fn contains(&self, other: Self) -> bool {
                    self.0.contains(other.0)
                }
                /// The bitwise or (`|`) of the bits in two flags values.
                #[inline]
                pub fn insert(&mut self, other: Self) {
                    self.0.insert(other.0)
                }
                /// The intersection of a source flags value with the complement of a target flags
                /// value (`&!`).
                ///
                /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
                /// `remove` won't truncate `other`, but the `!` operator will.
                #[inline]
                pub fn remove(&mut self, other: Self) {
                    self.0.remove(other.0)
                }
                /// The bitwise exclusive-or (`^`) of the bits in two flags values.
                #[inline]
                pub fn toggle(&mut self, other: Self) {
                    self.0.toggle(other.0)
                }
                /// Call `insert` when `value` is `true` or `remove` when `value` is `false`.
                #[inline]
                pub fn set(&mut self, other: Self, value: bool) {
                    self.0.set(other.0, value)
                }
                /// The bitwise and (`&`) of the bits in two flags values.
                #[inline]
                #[must_use]
                pub const fn intersection(self, other: Self) -> Self {
                    Self(self.0.intersection(other.0))
                }
                /// The bitwise or (`|`) of the bits in two flags values.
                #[inline]
                #[must_use]
                pub const fn union(self, other: Self) -> Self {
                    Self(self.0.union(other.0))
                }
                /// The intersection of a source flags value with the complement of a target flags
                /// value (`&!`).
                ///
                /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
                /// `difference` won't truncate `other`, but the `!` operator will.
                #[inline]
                #[must_use]
                pub const fn difference(self, other: Self) -> Self {
                    Self(self.0.difference(other.0))
                }
                /// The bitwise exclusive-or (`^`) of the bits in two flags values.
                #[inline]
                #[must_use]
                pub const fn symmetric_difference(self, other: Self) -> Self {
                    Self(self.0.symmetric_difference(other.0))
                }
                /// The bitwise negation (`!`) of the bits in a flags value, truncating the result.
                #[inline]
                #[must_use]
                pub const fn complement(self) -> Self {
                    Self(self.0.complement())
                }
            }
            impl ::bitflags::__private::core::fmt::Binary for Mask {
                fn fmt(
                    &self,
                    f: &mut ::bitflags::__private::core::fmt::Formatter,
                ) -> ::bitflags::__private::core::fmt::Result {
                    let inner = self.0;
                    ::bitflags::__private::core::fmt::Binary::fmt(&inner, f)
                }
            }
            impl ::bitflags::__private::core::fmt::Octal for Mask {
                fn fmt(
                    &self,
                    f: &mut ::bitflags::__private::core::fmt::Formatter,
                ) -> ::bitflags::__private::core::fmt::Result {
                    let inner = self.0;
                    ::bitflags::__private::core::fmt::Octal::fmt(&inner, f)
                }
            }
            impl ::bitflags::__private::core::fmt::LowerHex for Mask {
                fn fmt(
                    &self,
                    f: &mut ::bitflags::__private::core::fmt::Formatter,
                ) -> ::bitflags::__private::core::fmt::Result {
                    let inner = self.0;
                    ::bitflags::__private::core::fmt::LowerHex::fmt(&inner, f)
                }
            }
            impl ::bitflags::__private::core::fmt::UpperHex for Mask {
                fn fmt(
                    &self,
                    f: &mut ::bitflags::__private::core::fmt::Formatter,
                ) -> ::bitflags::__private::core::fmt::Result {
                    let inner = self.0;
                    ::bitflags::__private::core::fmt::UpperHex::fmt(&inner, f)
                }
            }
            impl ::bitflags::__private::core::ops::BitOr for Mask {
                type Output = Self;
                /// The bitwise or (`|`) of the bits in two flags values.
                #[inline]
                fn bitor(self, other: Mask) -> Self {
                    self.union(other)
                }
            }
            impl ::bitflags::__private::core::ops::BitOrAssign for Mask {
                /// The bitwise or (`|`) of the bits in two flags values.
                #[inline]
                fn bitor_assign(&mut self, other: Self) {
                    self.insert(other);
                }
            }
            impl ::bitflags::__private::core::ops::BitXor for Mask {
                type Output = Self;
                /// The bitwise exclusive-or (`^`) of the bits in two flags values.
                #[inline]
                fn bitxor(self, other: Self) -> Self {
                    self.symmetric_difference(other)
                }
            }
            impl ::bitflags::__private::core::ops::BitXorAssign for Mask {
                /// The bitwise exclusive-or (`^`) of the bits in two flags values.
                #[inline]
                fn bitxor_assign(&mut self, other: Self) {
                    self.toggle(other);
                }
            }
            impl ::bitflags::__private::core::ops::BitAnd for Mask {
                type Output = Self;
                /// The bitwise and (`&`) of the bits in two flags values.
                #[inline]
                fn bitand(self, other: Self) -> Self {
                    self.intersection(other)
                }
            }
            impl ::bitflags::__private::core::ops::BitAndAssign for Mask {
                /// The bitwise and (`&`) of the bits in two flags values.
                #[inline]
                fn bitand_assign(&mut self, other: Self) {
                    *self = Self::from_bits_retain(self.bits()).intersection(other);
                }
            }
            impl ::bitflags::__private::core::ops::Sub for Mask {
                type Output = Self;
                /// The intersection of a source flags value with the complement of a target flags value (`&!`).
                ///
                /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
                /// `difference` won't truncate `other`, but the `!` operator will.
                #[inline]
                fn sub(self, other: Self) -> Self {
                    self.difference(other)
                }
            }
            impl ::bitflags::__private::core::ops::SubAssign for Mask {
                /// The intersection of a source flags value with the complement of a target flags value (`&!`).
                ///
                /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
                /// `difference` won't truncate `other`, but the `!` operator will.
                #[inline]
                fn sub_assign(&mut self, other: Self) {
                    self.remove(other);
                }
            }
            impl ::bitflags::__private::core::ops::Not for Mask {
                type Output = Self;
                /// The bitwise negation (`!`) of the bits in a flags value, truncating the result.
                #[inline]
                fn not(self) -> Self {
                    self.complement()
                }
            }
            impl ::bitflags::__private::core::iter::Extend<Mask> for Mask {
                /// The bitwise or (`|`) of the bits in each flags value.
                fn extend<
                    T: ::bitflags::__private::core::iter::IntoIterator<Item = Self>,
                >(&mut self, iterator: T) {
                    for item in iterator {
                        self.insert(item)
                    }
                }
            }
            impl ::bitflags::__private::core::iter::FromIterator<Mask> for Mask {
                /// The bitwise or (`|`) of the bits in each flags value.
                fn from_iter<
                    T: ::bitflags::__private::core::iter::IntoIterator<Item = Self>,
                >(iterator: T) -> Self {
                    use ::bitflags::__private::core::iter::Extend;
                    let mut result = Self::empty();
                    result.extend(iterator);
                    result
                }
            }
            impl Mask {
                /// Yield a set of contained flags values.
                ///
                /// Each yielded flags value will correspond to a defined named flag. Any unknown bits
                /// will be yielded together as a final flags value.
                #[inline]
                pub const fn iter(&self) -> ::bitflags::iter::Iter<Mask> {
                    ::bitflags::iter::Iter::__private_const_new(
                        <Mask as ::bitflags::Flags>::FLAGS,
                        Mask::from_bits_retain(self.bits()),
                        Mask::from_bits_retain(self.bits()),
                    )
                }
                /// Yield a set of contained named flags values.
                ///
                /// This method is like [`iter`](#method.iter), except only yields bits in contained named flags.
                /// Any unknown bits, or bits not corresponding to a contained flag will not be yielded.
                #[inline]
                pub const fn iter_names(&self) -> ::bitflags::iter::IterNames<Mask> {
                    ::bitflags::iter::IterNames::__private_const_new(
                        <Mask as ::bitflags::Flags>::FLAGS,
                        Mask::from_bits_retain(self.bits()),
                        Mask::from_bits_retain(self.bits()),
                    )
                }
            }
            impl ::bitflags::__private::core::iter::IntoIterator for Mask {
                type Item = Mask;
                type IntoIter = ::bitflags::iter::Iter<Mask>;
                fn into_iter(self) -> Self::IntoIter {
                    self.iter()
                }
            }
        };
        ///Route `CONNECT` requests to the provided middleware.
        pub fn connect<T>(middleware: T) -> Branch<Allow<T>, Continue> {
            let mask = Mask::CONNECT;
            Branch {
                middleware: Allow { middleware, mask },
                or_else: Continue,
                mask,
            }
        }
        ///Route `DELETE` requests to the provided middleware.
        pub fn delete<T>(middleware: T) -> Branch<Allow<T>, Continue> {
            let mask = Mask::DELETE;
            Branch {
                middleware: Allow { middleware, mask },
                or_else: Continue,
                mask,
            }
        }
        ///Route `GET` requests to the provided middleware.
        pub fn get<T>(middleware: T) -> Branch<Allow<T>, Continue> {
            let mask = Mask::GET;
            Branch {
                middleware: Allow { middleware, mask },
                or_else: Continue,
                mask,
            }
        }
        ///Route `HEAD` requests to the provided middleware.
        pub fn head<T>(middleware: T) -> Branch<Allow<T>, Continue> {
            let mask = Mask::HEAD;
            Branch {
                middleware: Allow { middleware, mask },
                or_else: Continue,
                mask,
            }
        }
        ///Route `OPTIONS` requests to the provided middleware.
        pub fn options<T>(middleware: T) -> Branch<Allow<T>, Continue> {
            let mask = Mask::OPTIONS;
            Branch {
                middleware: Allow { middleware, mask },
                or_else: Continue,
                mask,
            }
        }
        ///Route `PATCH` requests to the provided middleware.
        pub fn patch<T>(middleware: T) -> Branch<Allow<T>, Continue> {
            let mask = Mask::PATCH;
            Branch {
                middleware: Allow { middleware, mask },
                or_else: Continue,
                mask,
            }
        }
        ///Route `POST` requests to the provided middleware.
        pub fn post<T>(middleware: T) -> Branch<Allow<T>, Continue> {
            let mask = Mask::POST;
            Branch {
                middleware: Allow { middleware, mask },
                or_else: Continue,
                mask,
            }
        }
        ///Route `PUT` requests to the provided middleware.
        pub fn put<T>(middleware: T) -> Branch<Allow<T>, Continue> {
            let mask = Mask::PUT;
            Branch {
                middleware: Allow { middleware, mask },
                or_else: Continue,
                mask,
            }
        }
        ///Route `TRACE` requests to the provided middleware.
        pub fn trace<T>(middleware: T) -> Branch<Allow<T>, Continue> {
            let mask = Mask::TRACE;
            Branch {
                middleware: Allow { middleware, mask },
                or_else: Continue,
                mask,
            }
        }
        impl<T, U> Branch<T, U> {
            pub fn connect<F>(self, middleware: F) -> Branch<Allow<F>, Self> {
                let mask = Mask::CONNECT;
                Branch {
                    mask: self.mask | mask,
                    or_else: self,
                    middleware: Allow { middleware, mask },
                }
            }
            pub fn delete<F>(self, middleware: F) -> Branch<Allow<F>, Self> {
                let mask = Mask::DELETE;
                Branch {
                    mask: self.mask | mask,
                    or_else: self,
                    middleware: Allow { middleware, mask },
                }
            }
            pub fn get<F>(self, middleware: F) -> Branch<Allow<F>, Self> {
                let mask = Mask::GET;
                Branch {
                    mask: self.mask | mask,
                    or_else: self,
                    middleware: Allow { middleware, mask },
                }
            }
            pub fn head<F>(self, middleware: F) -> Branch<Allow<F>, Self> {
                let mask = Mask::HEAD;
                Branch {
                    mask: self.mask | mask,
                    or_else: self,
                    middleware: Allow { middleware, mask },
                }
            }
            pub fn options<F>(self, middleware: F) -> Branch<Allow<F>, Self> {
                let mask = Mask::OPTIONS;
                Branch {
                    mask: self.mask | mask,
                    or_else: self,
                    middleware: Allow { middleware, mask },
                }
            }
            pub fn patch<F>(self, middleware: F) -> Branch<Allow<F>, Self> {
                let mask = Mask::PATCH;
                Branch {
                    mask: self.mask | mask,
                    or_else: self,
                    middleware: Allow { middleware, mask },
                }
            }
            pub fn post<F>(self, middleware: F) -> Branch<Allow<F>, Self> {
                let mask = Mask::POST;
                Branch {
                    mask: self.mask | mask,
                    or_else: self,
                    middleware: Allow { middleware, mask },
                }
            }
            pub fn put<F>(self, middleware: F) -> Branch<Allow<F>, Self> {
                let mask = Mask::PUT;
                Branch {
                    mask: self.mask | mask,
                    or_else: self,
                    middleware: Allow { middleware, mask },
                }
            }
            pub fn trace<F>(self, middleware: F) -> Branch<Allow<F>, Self> {
                let mask = Mask::TRACE;
                Branch {
                    mask: self.mask | mask,
                    or_else: self,
                    middleware: Allow { middleware, mask },
                }
            }
            /// Returns a `405 Method Not Allowed` response if the request method is
            /// not supported.
            ///
            /// # Example
            ///
            /// ```
            /// # use via::{Next, Request, Response, ResultExt};
            /// #
            /// # async fn greet(request: Request, _: Next) -> via::Result {
            /// #   let name = request.param("name").into_result()?;
            /// #   Response::build().text(format!("Hello, {}!", name))
            /// # }
            /// #
            /// # fn main() {
            /// # let mut app = via::app(());
            /// app.route("/hello/:name").to(via::get(greet).or_deny());
            /// // curl -XPOST http://localhost:8080/hello/world
            /// // => method not allowed: "POST"
            /// # }
            /// ```
            ///
            pub fn or_deny(self) -> Branch<Self, Deny> {
                let allow = self.mask;
                Branch {
                    middleware: self,
                    or_else: Deny { allow },
                    mask: allow,
                }
            }
        }
        impl Mask {
            fn as_str(&self) -> Option<&str> {
                match *self {
                    Mask::CONNECT => Some("CONNECT"),
                    Mask::DELETE => Some("DELETE"),
                    Mask::GET => Some("GET"),
                    Mask::HEAD => Some("HEAD"),
                    Mask::OPTIONS => Some("OPTIONS"),
                    Mask::PATCH => Some("PATCH"),
                    Mask::POST => Some("POST"),
                    Mask::PUT => Some("PUT"),
                    Mask::TRACE => Some("TRACE"),
                    _ => None,
                }
            }
        }
        impl MethodNotAllowed {
            pub(crate) fn allows(&self) -> Option<String> {
                self.allow
                    .iter()
                    .fold(
                        None,
                        |mut acc, mask| {
                            let Some(method) = mask.as_str() else {
                                return acc;
                            };
                            if let Some(allow) = acc.as_mut() {
                                allow.push_str(", ");
                                allow.push_str(method);
                            } else {
                                acc = Some(String::with_capacity(64) + method);
                            }
                            acc
                        },
                    )
            }
        }
        impl<T> Predicate for Allow<T> {
            #[inline]
            fn matches(&self, other: &Mask) -> bool {
                self.mask.contains(*other)
            }
        }
        impl<T, U> Predicate for Branch<T, U> {
            #[inline]
            fn matches(&self, other: &Mask) -> bool {
                self.mask.contains(*other)
            }
        }
        impl<T, App> Middleware<App> for Allow<T>
        where
            T: Middleware<App>,
        {
            fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture {
                self.middleware.call(request, next)
            }
        }
        impl<T, OrElse, App> Middleware<App> for Branch<T, OrElse>
        where
            T: Middleware<App> + Predicate,
            OrElse: Middleware<App>,
        {
            #[inline(always)]
            fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture {
                let mask = request.method().into();
                if self.middleware.matches(&mask) {
                    self.middleware.call(request, next)
                } else {
                    self.or_else.call(request, next)
                }
            }
        }
        impl<App> Middleware<App> for Deny {
            fn call(&self, request: Request<App>, _: Next<App>) -> BoxFuture {
                let error = Error::method_not_allowed(MethodNotAllowed {
                    allow: self.allow,
                    method: request.envelope().method().into(),
                });
                Box::pin(async { Err(error) })
            }
        }
        impl From<&'_ http::Method> for Mask {
            fn from(method: &http::Method) -> Self {
                match *method {
                    http::Method::CONNECT => Mask::CONNECT,
                    http::Method::DELETE => Mask::DELETE,
                    http::Method::GET => Mask::GET,
                    http::Method::HEAD => Mask::HEAD,
                    http::Method::OPTIONS => Mask::OPTIONS,
                    http::Method::PATCH => Mask::PATCH,
                    http::Method::POST => Mask::POST,
                    http::Method::PUT => Mask::PUT,
                    http::Method::TRACE => Mask::TRACE,
                    _ => Mask::empty(),
                }
            }
        }
        impl std::error::Error for MethodNotAllowed {}
        impl Display for MethodNotAllowed {
            fn fmt(&self, f: &mut Formatter) -> fmt::Result {
                if let Some(method) = self.method.as_str() {
                    f.write_fmt(format_args!("method not allowed: \"{0}\"", method))
                } else {
                    f.write_fmt(format_args!("method not allowed"))
                }
            }
        }
    }
    mod route {
        use std::sync::Arc;
        use via_router::RouteMut;
        use super::allow::{Branch, Deny};
        use crate::middleware::Middleware;
        /// A reborrow of a `&mut Route<App>` that can define a "responder" middleware.
        pub struct Index<'a, App>(Route<'a, App>);
        /// An entry in the route tree associated with a path segment pattern.
        ///
        /// Route definitions are composable and inherit middleware from their
        /// ancestors. The order in which routes and their middleware are defined
        /// determines the sequence of operations that occur when a user visits a given
        /// route.
        ///
        /// A well-structured application strategically defines middleware so that
        /// shared behavior is expressed by a common path segment prefix and ordered
        /// to reflect its execution sequence.
        ///
        /// # Example
        ///
        /// ```no_run
        /// use std::process::ExitCode;
        /// use via::{Error, Next, Request, Server, rescue};
        ///
        /// #[tokio::main]
        /// async fn main() -> Result<ExitCode, Error> {
        ///     let mut app = via::app(());
        ///     let mut api = app.route("/api");
        ///
        ///     // If an error occurs on a descendant of /api, respond with json.
        ///     // Siblings of /api must define their own error handling logic.
        ///     api.middleware(rescue(|sanitizer| sanitizer.use_json()));
        ///
        ///     // Define a /users resource as a child of /api so the rescue and timeout
        ///     // middleware run before any of the middleware or responders defined in
        ///     // the /users resource.
        ///     api.route("/users").scope(|users| {
        ///         let index = async |_, _| todo!();
        ///         let show = async |_, _| todo!();
        ///
        ///         // list users
        ///         users.route("/").to(via::get(index));
        ///
        ///         // find user with id = :id
        ///         users.route("/:id").to(via::get(show));
        ///     });
        ///
        ///     // Start serving our application from http://localhost:8080/.
        ///     Server::new(app).listen(("127.0.0.1", 8080)).await
        /// }
        /// ```
        pub struct Route<'a, App>(pub(super) RouteMut<'a, Arc<dyn Middleware<App>>>);
        /// Describes a RESTful resource by associating path segments with middleware.
        ///
        /// A [`Resource`] can be constructed by passing an identifier to a module as well as
        /// a parameter name to the [`rest!`](crate::rest) macro.
        ///
        /// # Example
        ///
        /// ```
        /// use routes::{channels, reactions, threads};
        /// use via::rest;
        ///
        /// // A chat application.
        /// let mut chat = via::app(());
        ///
        /// // Our chat application nests routes that return data under an /api prefix.
        /// let mut api = chat.route("/api");
        ///
        /// // Let `channel` be an entry to: /api/channels/:channel-id.
        /// let mut channel = api.resource(rest!(channels, ":channel-id"));
        /// //                             ^^^^
        /// // Source CRUD actions from routes::channels and define the following:
        /// //   - GET /api/channels/:channel-id ~> channels::index
        /// //   - POST /api/channels/:channel-id ~> channels::create
        /// //   - GET /api/channels/:channel-id ~> channels::show
        /// //   - PATCH /api/channels/:channel-id ~> channels::update
        /// //   - DELETE /api/channels/:channel-id ~> channels::destroy
        /// //
        /// // The `rest!` macro collapses channels::{index, create} and
        /// // channels::{destroy, show, update} into 2 respective middlewares.
        /// //
        /// // This amortizes the routing cost of HTTP method-based dispatch into a
        /// // single Arc::clone per request rather than a clone per method.
        ///
        /// // Now we can define the resources that are owned by a channel as
        /// // descendants of `channel`.
        /// channel.scope(|channel| {
        ///     // Let `thread` be an entry to: ./threads/:thread-id.
        ///     let mut thread = channel.resource(rest!(threads, ":thread-id"));
        ///
        ///     // Since a reply is simply a thread that belongs to a thread we can use
        ///     // the routes::threads module to define the replies resource.
        ///     //
        ///     // Let `reply` be an entry to: ./threads/:thread-id/replies/:reply-id.
        ///     let mut reply = thread.resource(rest!(threads, ":reply-id", "replies"));
        ///
        ///     // Since `reply` is holding a mutable borrow to `thread`, we must first
        ///     // define the reactions resource on `reply` to prevent a compile error
        ///     // from the borrow checker.
        ///     reply.resource(rest!(reactions, ":reaction-id"));
        ///
        ///     // Now we can define the reactions resource on `thread` since `reply`
        ///     // is no longer referenced in this scope.
        ///     thread.resource(rest!(reactions, ":reaction-id"));
        /// });
        /// #
        /// # macro_rules! action {
        /// #     ($name:ident) => {
        /// #         pub async fn $name(_: via::Request, _: via::Next) -> via::Result { todo!() }
        /// #     };
        /// # }
        /// #
        /// # macro_rules! resource {
        /// #     ($($name:ident),*) => {
        /// #         $(pub mod $name {
        /// #             action!(index);
        /// #             action!(create);
        /// #             action!(show);
        /// #             action!(update);
        /// #             action!(destroy);
        /// #         })*
        /// #     };
        /// # }
        /// #
        /// # mod routes {
        /// #     resource!(channels, reactions, threads);
        /// # }
        /// ```
        pub struct Resource<T, U> {
            collection: WithPath<T>,
            member: WithPath<U>,
        }
        #[doc(hidden)]
        pub struct ResourceBuilder<T> {
            collection: WithPath<T>,
        }
        struct WithPath<T> {
            path: &'static str,
            middleware: T,
        }
        impl<'a, App> Index<'a, App> {
            /// Defines how the route should respond when it is visited.
            ///
            /// Unlike [`Route::to`], the mutable borrow to the route tree entry is
            /// consumed and not returned. This prevents logical aliasing errors that
            /// can occur when defining a tree-like structure with a builder style API.
            pub fn to<T>(self, middleware: T)
            where
                T: Middleware<App> + 'static,
            {
                self.0.to(middleware);
            }
        }
        impl<'a, App> Route<'a, App> {
            /// Reborrow `self` in order to define a "responder" middleware.
            ///
            /// Route resolution is dependent on the sequence in which they are
            /// defined. In order to encourage clean code and a linear progression of
            /// route definitons, [`Self::to`] takes ownership of `self`. This prevents
            /// descendants from implicitly defining routes on ancestors. [`Self::to`]
            /// returns `self` to support builder-style method chains that allow you to
            /// continue nesting descendant routes from an ancestor.
            ///
            /// Sometimes it can be beneficial to temporarily opt-out of the typical
            /// flow of the router DSL. A common pattern used when defining routes at
            /// scale is moving a route definition at the root of your application's
            /// main fn directly into a scope. This allows common identifiers to be
            /// reused without shadowing identifier names of sibling route definitions.
            ///
            /// # Example
            ///
            /// ```
            /// use via::{Next, Request};
            ///
            /// let mut app = via::app(());
            /// let mut api = app.route("/api");
            ///
            /// api.route("/users").scope(|users| {
            ///     users.middleware(async |request: Request, next: Next| {
            ///         // Confirm that the request is authenticated.
            ///         // Then, call the next middleware.
            ///         next.call(request).await
            ///     });
            ///
            ///     let list = via::get(async |request: Request, _: Next| {
            ///         todo!("Respond with a list of users");
            ///     });
            ///
            ///     let show = via::get(async |request: Request, _: Next| {
            ///         todo!("Respond with the user with id = :user-id.");
            ///     });
            ///
            ///     // Reborrow `users` so it can be consumed by `.to(..)`.
            ///     users.index().to(list);
            ///
            ///     // The mutable borrow to `users` is still live.
            ///     users.route("/:user-id").to(show).scope(|user| {
            ///         // Define descendents from /api/users/:user-id.
            ///     });
            /// });
            /// ```
            pub fn index(&mut self) -> Index<'_, App> {
                Index(self.route("/"))
            }
            /// Appends the provided middleware to the route's call stack.
            ///
            /// Middleware attached to a route runs anytime the route’s path is a
            /// prefix of the request path.
            ///
            /// # Example
            ///
            /// ```
            /// # use via::{Next, Request, raise};
            /// # let mut app = via::app(());
            /// #
            /// // Provides application-wide support for request and response cookies.
            /// app.middleware(via::cookies(["is-admin"]));
            ///
            /// // Requests made to /admin or any of its descendants must have an
            /// // is-admin cookie present on the request.
            /// app.route("/admin").middleware(async |request: Request, next: Next| {
            ///     // We suggest using signed cookies to prevent tampering.
            ///     // See the cookies example in our git repo for more information.
            ///     if request.envelope().cookies().get("is-admin").is_none() {
            ///         raise!(401);
            ///     }
            ///
            ///     next.call(request).await
            /// });
            /// ```
            ///
            pub fn middleware<T>(&mut self, middleware: T)
            where
                T: Middleware<App> + 'static,
            {
                self.0.middleware(Arc::new(middleware));
            }
            /// Mount the provided RESTful resource at `self` and return a mutable
            /// borrow to the "member" route.
            pub fn resource<T, U>(&mut self, resource: Resource<T, U>) -> Route<'_, App>
            where
                T: Middleware<App> + 'static,
                U: Middleware<App> + 'static,
            {
                self.route(resource.collection.path).to(resource.collection.middleware);
                self.route(resource.member.path).to(resource.member.middleware)
            }
            /// Returns a new child route by appending the provided path to the current
            /// route.
            ///
            /// The path argument can contain multiple segments. The returned route
            /// always represents the final segment of that path.
            ///
            /// # Example
            ///
            /// ```
            /// # let mut app = via::app(());
            /// // The following routes reference the router entry at /hello/:name.
            /// app.route("/hello/:name");
            /// app.route("/hello").route("/:name");
            /// ```
            ///
            /// # Dynamic Segments
            ///
            /// Routes can include *dynamic* segments that capture portions of the
            /// request path as parameters. These parameters are made available to
            /// middleware at runtime.
            ///
            /// - `:dynamic` — Matches a single path segment. `/users/:id` matches
            ///   `/users/12345` and captures `"12345"` as `id`.
            ///
            /// - `*splat` — Matches zero or more remaining path segments.
            ///   `/static/*asset` matches `/static/logo.png` or `/static/css/main.css`
            ///   and captures the remainder of the path starting from the splat
            ///   pattern as `asset`. `logo.png` and `css/main.css`.
            ///
            /// Dynamic segments match any path segment, so define them after all
            /// static sibling routes to ensure intended routing behavior.
            ///
            /// Consider the following sequence of route definitions. We define
            /// `/articles/trending` before `/articles/:id` to ensure that a request to
            /// `/articles/trending` is routed to `articles::trending` rather than
            /// capturing `"trending"` as `id` and invoking `articles::show`.
            ///
            /// ```
            /// # let mut app = via::app(());
            /// let mut resource = app.route("/posts");
            ///
            /// resource.route("/").to(via::get(posts::index));
            /// resource.route("/:id").to(via::get(posts::show));
            /// resource.route("/trending").to(via::get(posts::trending));
            /// #
            /// # mod posts {
            /// #     use via::{Next, Request};
            /// #     pub async fn trending(_: Request, _: Next) -> via::Result { todo!() }
            /// #     pub async fn index(_: Request, _: Next) -> via::Result { todo!() }
            /// #     pub async fn show(_: Request, _: Next) -> via::Result { todo!() }
            /// # }
            /// ```
            ///
            pub fn route(&mut self, path: &'static str) -> Route<'_, App> {
                Route(self.0.route(path))
            }
            /// Consumes self by calling the provided closure with a mutable reference
            /// to self.
            ///
            pub fn scope(mut self, scope: impl FnOnce(&mut Self)) {
                scope(&mut self);
            }
            /// Defines how the route should respond when it is visited.
            ///
            /// # Example
            ///
            /// ```
            /// # mod users {
            /// #     use via::{Next, Request};
            /// #     pub async fn index(_: Request, _: Next) -> via::Result { todo!() }
            /// #     pub async fn show(_: Request, _: Next) -> via::Result { todo!() }
            /// # }
            /// #
            /// # let mut app = via::app(());
            /// #
            /// // Called only when the request path is /users.
            /// let mut users = app.route("/users").to(via::get(users::show));
            ///
            /// // Called only when the request path matches /users/:id.
            /// users.route("/:id").to(via::get(users::show));
            /// ```
            ///
            pub fn to<T>(self, middleware: T) -> Self
            where
                T: Middleware<App> + 'static,
            {
                Self(self.0.to(Arc::new(middleware)))
            }
        }
        impl<T, U, X, Y> Resource<Branch<T, X>, Branch<U, Y>> {
            /// Returns a `405 Method Not Allowed` response if the request method is
            /// not supported by the resource.
            #[allow(clippy::type_complexity)]
            pub fn or_deny(
                self,
            ) -> Resource<Branch<Branch<T, X>, Deny>, Branch<Branch<U, Y>, Deny>> {
                Resource {
                    collection: WithPath {
                        path: self.collection.path,
                        middleware: self.collection.middleware.or_deny(),
                    },
                    member: WithPath {
                        path: self.member.path,
                        middleware: self.member.middleware.or_deny(),
                    },
                }
            }
        }
        #[doc(hidden)]
        impl<T> ResourceBuilder<T> {
            pub fn collection(path: &'static str, middleware: T) -> ResourceBuilder<T> {
                ResourceBuilder {
                    collection: WithPath { path, middleware },
                }
            }
            pub fn member<U>(self, path: &'static str, middleware: U) -> Resource<T, U> {
                Resource {
                    collection: self.collection,
                    member: WithPath { path, middleware },
                }
            }
        }
    }
    pub use allow::*;
    pub use route::{Index, Resource, ResourceBuilder, Route};
    pub(crate) use allow::MethodNotAllowed;
    use std::sync::Arc;
    use via_router::Traverse;
    use crate::middleware::Middleware;
    pub(crate) struct Router<T> {
        tree: via_router::Router<Arc<dyn Middleware<T>>>,
    }
    impl<T> Router<T> {
        pub fn new() -> Self {
            Self {
                tree: via_router::Router::new(),
            }
        }
        pub fn route(&mut self, path: &'static str) -> Route<'_, T> {
            Route(self.tree.route(path))
        }
        pub fn traverse<'b>(
            &self,
            path: &'b str,
        ) -> Traverse<'_, 'b, Arc<dyn Middleware<T>>> {
            self.tree.traverse(path)
        }
    }
}
pub mod ws {
    mod channel {
        pub use tungstenite::protocol::{CloseFrame, Message, frame::Utf8Bytes};
        use futures_channel::mpsc::{self, Receiver, Sender, TryRecvError};
        use std::future::Future;
        use std::pin::Pin;
        use std::task::{Context, Poll};
        use super::error::already_closed;
        use super::util::poll_immediate_no_wake;
        pub struct Channel {
            tx: Sender<Message>,
            rx: Receiver<Message>,
        }
        struct Send<'a> {
            sender: &'a mut Sender<Message>,
            message: Option<Message>,
        }
        struct Recv<'a> {
            recv: mpsc::Recv<'a, mpsc::Receiver<Message>>,
        }
        fn poll_ready(
            tx: &mut Sender<Message>,
            cx: &mut Context,
        ) -> Poll<super::Result> {
            match tx.poll_ready(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Ok(_)) => Poll::Ready(Ok(())),
                Poll::Ready(Err(error)) => {
                    if error.is_disconnected() {
                        Poll::Ready(Err(already_closed()))
                    } else {
                        Poll::Pending
                    }
                }
            }
        }
        impl Channel {
            pub fn send(
                &mut self,
                message: impl Into<Message>,
            ) -> impl Future<Output = super::Result> {
                Send {
                    sender: &mut self.tx,
                    message: Some(message.into()),
                }
            }
            pub fn recv(&mut self) -> impl Future<Output = Option<Message>> {
                Recv { recv: self.rx.recv() }
            }
        }
        impl Channel {
            pub(super) fn new() -> (Self, Self) {
                let (tx1, rx2) = mpsc::channel(0);
                let (tx2, rx1) = mpsc::channel(0);
                (Self { tx: tx1, rx: rx1 }, Self { tx: tx2, rx: rx2 })
            }
            /// Check the capacity of the channel without registering a wake.
            pub(super) fn has_capacity(&mut self) -> super::Result<bool> {
                match poll_immediate_no_wake(|noop| poll_ready(&mut self.tx, noop)) {
                    Poll::Ready(result) => result.and(Ok(true)),
                    Poll::Pending => Ok(false),
                }
            }
            /// Try to receive the next message without registering a wake.
            pub(super) fn try_recv(&mut self) -> super::Result<Option<Message>> {
                match self.rx.try_recv() {
                    Ok(outbound) => Ok(Some(outbound)),
                    Err(TryRecvError::Empty) => Ok(None),
                    Err(TryRecvError::Closed) => Err(already_closed()),
                }
            }
            /// Try to send the provided message without registering a wake.
            pub(super) fn try_send(&mut self, message: Message) -> super::Result {
                self.tx
                    .try_send(message)
                    .map_err(|error| {
                        if true {
                            if !error.into_send_error().is_full() {
                                {
                                    ::core::panicking::panic_fmt(
                                        format_args!(
                                            "via::ws::Channel::try_send(..) requires a readiness check.",
                                        ),
                                    );
                                }
                            }
                        }
                        already_closed()
                    })
            }
        }
        impl Future for Recv<'_> {
            type Output = Option<Message>;
            fn poll(mut self: Pin<&mut Self>, _: &mut Context) -> Poll<Self::Output> {
                poll_immediate_no_wake(|noop| Pin::new(&mut self.recv).poll(noop))
                    .map(Result::ok)
            }
        }
        impl Future for Send<'_> {
            type Output = super::Result;
            fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
                if poll_ready(self.sender, cx)?.is_pending() {
                    return Poll::Pending;
                }
                let Some(message) = self.message.take() else {
                    return Poll::Ready(Ok(()));
                };
                match self.sender.try_send(message) {
                    Ok(_) => Poll::Ready(Ok(())),
                    Err(error) => {
                        if error.is_disconnected() {
                            Poll::Ready(Err(already_closed()))
                        } else {
                            Poll::Pending
                        }
                    }
                }
            }
        }
    }
    mod error {
        use std::fmt::{self, Display, Formatter};
        use std::ops::ControlFlow;
        use crate::error::Error;
        use crate::guard;
        use http::header::{CONNECTION, UPGRADE};
        pub use tungstenite::error::Error as WebSocketError;
        pub type Result<T = ()> = std::result::Result<T, ControlFlow<Error, Error>>;
        pub trait ResultExt {
            type Output;
            fn or_close(self) -> Result<Self::Output>;
            fn or_reconnect(self) -> Result<Self::Output>;
        }
        pub enum UpgradeError {
            InvalidAcceptEncoding,
            InvalidConnectionHeader,
            InvalidUpgradeHeader,
            EncoderError,
            MissingAcceptKey,
            UnknownVersion,
            Unsupported,
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for UpgradeError {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::write_str(
                    f,
                    match self {
                        UpgradeError::InvalidAcceptEncoding => "InvalidAcceptEncoding",
                        UpgradeError::InvalidConnectionHeader => {
                            "InvalidConnectionHeader"
                        }
                        UpgradeError::InvalidUpgradeHeader => "InvalidUpgradeHeader",
                        UpgradeError::EncoderError => "EncoderError",
                        UpgradeError::MissingAcceptKey => "MissingAcceptKey",
                        UpgradeError::UnknownVersion => "UnknownVersion",
                        UpgradeError::Unsupported => "Unsupported",
                    },
                )
            }
        }
        pub fn already_closed() -> ControlFlow<Error, Error> {
            ControlFlow::Break(Error::other(Box::new(WebSocketError::AlreadyClosed)))
        }
        pub fn rescue(error: WebSocketError) -> ControlFlow<Error, Error> {
            use std::io::ErrorKind;
            if let WebSocketError::Io(io) = &error
                && let ErrorKind::Interrupted | ErrorKind::TimedOut = io.kind()
            {
                ControlFlow::Continue(error.into())
            } else {
                ControlFlow::Break(error.into())
            }
        }
        impl std::error::Error for UpgradeError {}
        impl Display for UpgradeError {
            fn fmt(&self, f: &mut Formatter) -> fmt::Result {
                match self {
                    UpgradeError::InvalidAcceptEncoding => {
                        f.write_fmt(
                            format_args!(
                                "header \"sec-websocket-key\" must be base64 encoded.",
                            ),
                        )
                    }
                    UpgradeError::InvalidConnectionHeader => {
                        f.write_fmt(
                            format_args!(
                                "\"connection\" header must contain token: \"upgrade\".",
                            ),
                        )
                    }
                    UpgradeError::InvalidUpgradeHeader => {
                        f.write_fmt(
                            format_args!(
                                "\"upgrade\" header must contain token: \"websocket\".",
                            ),
                        )
                    }
                    UpgradeError::EncoderError => {
                        f.write_fmt(
                            format_args!("failed to encode \"sec-websocket-accept\"."),
                        )
                    }
                    UpgradeError::MissingAcceptKey => {
                        f.write_fmt(
                            format_args!(
                                "missing required header: \"sec-websocket-key\".",
                            ),
                        )
                    }
                    UpgradeError::UnknownVersion => {
                        f.write_fmt(
                            format_args!("\"sec-websocket-version\" must be \"13\"."),
                        )
                    }
                    UpgradeError::Unsupported => {
                        f.write_fmt(
                            format_args!(
                                "connection does not support websocket upgrades.",
                            ),
                        )
                    }
                }
            }
        }
        impl From<guard::ErrorKind> for UpgradeError {
            fn from(error: guard::ErrorKind) -> Self {
                match error {
                    guard::ErrorKind::Header(CONNECTION) => {
                        UpgradeError::InvalidConnectionHeader
                    }
                    guard::ErrorKind::Header(UPGRADE) => {
                        UpgradeError::InvalidUpgradeHeader
                    }
                    _ => UpgradeError::UnknownVersion,
                }
            }
        }
        impl<T, E> ResultExt for std::result::Result<T, E>
        where
            Error: From<E>,
        {
            type Output = T;
            #[inline]
            fn or_close(self) -> Result<Self::Output> {
                self.map_err(|error| ControlFlow::Break(error.into()))
            }
            #[inline]
            fn or_reconnect(self) -> Result<Self::Output> {
                self.map_err(|error| ControlFlow::Continue(error.into()))
            }
        }
    }
    mod io {
        use hyper::upgrade::Upgraded;
        use hyper_util::rt::TokioIo;
        use std::io;
        use std::pin::Pin;
        use std::task::{Context, Poll};
        use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
        pub struct UpgradedIo {
            io: TokioIo<Upgraded>,
        }
        impl UpgradedIo {
            #[inline]
            pub fn new(io: Upgraded) -> Self {
                Self { io: TokioIo::new(io) }
            }
        }
        impl AsyncRead for UpgradedIo {
            fn poll_read(
                mut self: Pin<&mut Self>,
                context: &mut Context<'_>,
                buf: &mut ReadBuf<'_>,
            ) -> Poll<io::Result<()>> {
                Pin::new(&mut self.io).poll_read(context, buf)
            }
        }
        impl AsyncWrite for UpgradedIo {
            fn poll_write(
                mut self: Pin<&mut Self>,
                context: &mut Context<'_>,
                buf: &[u8],
            ) -> Poll<io::Result<usize>> {
                Pin::new(&mut self.io).poll_write(context, buf)
            }
            fn poll_flush(
                mut self: Pin<&mut Self>,
                context: &mut Context<'_>,
            ) -> Poll<io::Result<()>> {
                Pin::new(&mut self.io).poll_flush(context)
            }
            fn poll_shutdown(
                mut self: Pin<&mut Self>,
                context: &mut Context<'_>,
            ) -> Poll<io::Result<()>> {
                Pin::new(&mut self.io).poll_shutdown(context)
            }
            fn is_write_vectored(&self) -> bool {
                self.io.is_write_vectored()
            }
            fn poll_write_vectored(
                mut self: Pin<&mut Self>,
                context: &mut Context<'_>,
                bufs: &[io::IoSlice<'_>],
            ) -> Poll<io::Result<usize>> {
                Pin::new(&mut self.io).poll_write_vectored(context, bufs)
            }
        }
        impl Drop for UpgradedIo {
            fn drop(&mut self) {}
        }
    }
    mod request {
        use std::sync::Arc;
        use crate::app::Shared;
        use crate::request::Envelope;
        pub struct Request<App = ()> {
            envelope: Arc<Envelope>,
            app: Shared<App>,
        }
        #[automatically_derived]
        impl<App: ::core::fmt::Debug> ::core::fmt::Debug for Request<App> {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::debug_struct_field2_finish(
                    f,
                    "Request",
                    "envelope",
                    &self.envelope,
                    "app",
                    &&self.app,
                )
            }
        }
        impl<App> Request<App> {
            pub fn app(&self) -> &App {
                &self.app
            }
            pub fn envelope(&self) -> &Envelope {
                &self.envelope
            }
            pub fn app_owned(&self) -> Shared<App> {
                self.app.clone()
            }
        }
        impl<App> Request<App> {
            pub(super) fn new(request: crate::Request<App>) -> Self {
                let (envelope, _, app) = request.into_parts();
                Self {
                    envelope: Arc::new(envelope),
                    app,
                }
            }
        }
        impl<App> Clone for Request<App> {
            fn clone(&self) -> Self {
                Self {
                    envelope: Arc::clone(&self.envelope),
                    app: self.app.clone(),
                }
            }
        }
    }
    mod run {
        use futures_core::Stream;
        use futures_sink::Sink;
        use std::future::Future;
        use std::marker::PhantomPinned;
        use std::mem;
        use std::ops::ControlFlow;
        use std::pin::Pin;
        use std::sync::Arc;
        use std::task::{Context, Poll};
        use tokio_tungstenite::WebSocketStream;
        use super::error::rescue;
        use super::io::UpgradedIo;
        use super::{Channel, Message, Request};
        use crate::Error;
        use crate::ws::upgrade::Listener;
        pub struct RunTask<T, App> {
            run: Pin<Box<Run<T, App>>>,
        }
        enum IoState {
            Receive,
            Send(Message),
            Flush,
        }
        struct Facade {
            listener: Pin<Box<dyn Future<Output = super::Result> + Send>>,
            state: IoState,
            stream: *mut WebSocketStream<UpgradedIo>,
            rendezvous: Channel,
        }
        struct Run<T, App> {
            listener: Arc<Listener<T>>,
            request: Request<App>,
            stream: WebSocketStream<UpgradedIo>,
            facade: Option<Facade>,
            _pin: PhantomPinned,
        }
        impl<T, App, Await> RunTask<T, App>
        where
            T: Fn(Channel, Request<App>) -> Await + Send,
            Await: Future<Output = super::Result> + Send + 'static,
        {
            pub(super) fn new(
                listener: Arc<Listener<T>>,
                request: Request<App>,
                stream: WebSocketStream<UpgradedIo>,
            ) -> Self {
                Self {
                    run: Box::pin(Run {
                        listener,
                        request,
                        stream,
                        facade: None,
                        _pin: PhantomPinned,
                    }),
                }
            }
        }
        impl<T, App, Await> Future for RunTask<T, App>
        where
            T: Fn(Channel, Request<App>) -> Await + Send,
            Await: Future<Output = super::Result> + Send + 'static,
        {
            type Output = Result<(), Error>;
            fn poll(
                mut self: Pin<&mut Self>,
                context: &mut Context,
            ) -> Poll<Self::Output> {
                self.run.as_mut().poll(context)
            }
        }
        unsafe impl Send for Facade {}
        impl Drop for Facade {
            fn drop(&mut self) {
                self.stream = std::ptr::null_mut();
            }
        }
        impl Future for Facade {
            type Output = super::Result;
            fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
                loop {
                    let stream = unsafe { &mut *self.stream };
                    match &mut self.state {
                        IoState::Receive => {
                            if true {
                                {
                                    ::std::io::_eprint(
                                        format_args!(
                                            "{0}info(via::ws): state = receive\n",
                                            " ".repeat(0),
                                        ),
                                    );
                                };
                            }
                            if self.rendezvous.has_capacity()? {
                                if true {
                                    {
                                        ::std::io::_eprint(
                                            format_args!(
                                                "{0}info(via::ws): listener is ready for the next message.\n",
                                                " ".repeat(2),
                                            ),
                                        );
                                    };
                                }
                                if let Poll::Ready(next) = Pin::new(stream)
                                    .poll_next(cx)
                                    .map_err(rescue)?
                                {
                                    let Some(received) = next else {
                                        return Poll::Ready(Ok(()));
                                    };
                                    self.rendezvous.try_send(received)?;
                                    if true {
                                        {
                                            ::std::io::_eprint(
                                                format_args!(
                                                    "{0}info(via::ws): message received.\n",
                                                    " ".repeat(4),
                                                ),
                                            );
                                        };
                                    }
                                }
                            } else {
                                if true {
                                    {
                                        ::std::io::_eprint(
                                            format_args!(
                                                "{0}info(via::ws): listener has not received the previous message.\n",
                                                " ".repeat(2),
                                            ),
                                        );
                                    };
                                }
                            }
                            if self.listener.as_mut().poll(cx)?.is_ready() {
                                return Poll::Ready(Ok(()));
                            }
                            if let Some(sent) = self.rendezvous.try_recv()? {
                                if true {
                                    {
                                        ::std::io::_eprint(
                                            format_args!(
                                                "{0}info(via::ws): listener sent a message\n",
                                                " ".repeat(4),
                                            ),
                                        );
                                    };
                                }
                                self.state = IoState::Send(sent);
                            } else {
                                if true {
                                    {
                                        ::std::io::_eprint(
                                            format_args!(
                                                "{0}info(via::ws): wake for the next message or listener progress.\n",
                                                " ".repeat(4),
                                            ),
                                        );
                                    };
                                }
                                return Poll::Pending;
                            }
                        }
                        state @ IoState::Send(_) => {
                            if true {
                                {
                                    ::std::io::_eprint(
                                        format_args!(
                                            "{0}info(via::ws): state = send\n",
                                            " ".repeat(0),
                                        ),
                                    );
                                };
                            }
                            let mut sink = Pin::new(stream);
                            if sink.as_mut().poll_ready(cx).map_err(rescue)?.is_pending()
                            {
                                if true {
                                    {
                                        ::std::io::_eprint(
                                            format_args!(
                                                "{0}info(via::ws): waiting for i/o to become available for write.\n",
                                                " ".repeat(2),
                                            ),
                                        );
                                    };
                                }
                                return Poll::Pending;
                            }
                            if let IoState::Send(message) = mem::replace(
                                state,
                                IoState::Flush,
                            ) {
                                sink.as_mut().start_send(message).map_err(rescue)?;
                                if true {
                                    {
                                        ::std::io::_eprint(
                                            format_args!(
                                                "{0}info(via::ws): write successful. transitiion to flush.\n",
                                                " ".repeat(2),
                                            ),
                                        );
                                    };
                                }
                            } else {
                                return Poll::Ready(Ok(()));
                            }
                        }
                        IoState::Flush => {
                            if true {
                                {
                                    ::std::io::_eprint(
                                        format_args!(
                                            "{0}info(via::ws): state = flush\n",
                                            " ".repeat(0),
                                        ),
                                    );
                                };
                            }
                            if Pin::new(stream)
                                .poll_flush(cx)
                                .map_err(rescue)?
                                .is_ready()
                            {
                                if true {
                                    {
                                        ::std::io::_eprint(
                                            format_args!(
                                                "{0}info(via::ws): flush complete. transition to receive.\n",
                                                " ".repeat(2),
                                            ),
                                        );
                                    };
                                }
                                self.state = IoState::Receive;
                            } else {
                                if true {
                                    {
                                        ::std::io::_eprint(
                                            format_args!(
                                                "{0}info(via::ws): waiting for flush to complete.\n",
                                                " ".repeat(2),
                                            ),
                                        );
                                    };
                                }
                                return Poll::Pending;
                            }
                        }
                    }
                }
            }
        }
        impl<T, App, Await> Run<T, App>
        where
            T: Fn(Channel, Request<App>) -> Await + Send,
            Await: Future<Output = super::Result> + Send + 'static,
        {
            #[inline(always)]
            fn reconnect(&mut self) -> &mut Facade {
                let (ours, theirs) = Channel::new();
                let request = self.request.clone();
                let facade = Facade {
                    listener: Box::pin((self.listener.handle)(theirs, request)),
                    state: IoState::Receive,
                    stream: &mut self.stream as *mut _,
                    rendezvous: ours,
                };
                self.facade = Some(facade);
                unsafe { self.facade.as_mut().unwrap_unchecked() }
            }
        }
        impl<T, App> Drop for Run<T, App> {
            fn drop(&mut self) {
                if let Some(facade) = self.facade.take() {
                    drop(facade);
                }
            }
        }
        impl<T, App, Await> Future for Run<T, App>
        where
            T: Fn(Channel, Request<App>) -> Await + Send,
            Await: Future<Output = super::Result> + Send + 'static,
        {
            type Output = Result<(), Error>;
            fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
                let this = unsafe { self.get_unchecked_mut() };
                let future = match this.facade.as_mut() {
                    Some(facade) => facade,
                    None => this.reconnect(),
                };
                match Pin::new(future).poll(context) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(Ok(_)) => Poll::Ready(Ok(())),
                    Poll::Ready(Err(ControlFlow::Break(error))) => {
                        if true {
                            {
                                ::std::io::_eprint(
                                    format_args!(
                                        "{0}error(via::ws): {1}\n",
                                        " ".repeat(2),
                                        &error,
                                    ),
                                );
                            };
                        }
                        Poll::Ready(Err(error))
                    }
                    Poll::Ready(Err(ControlFlow::Continue(error))) => {
                        if true {
                            {
                                ::std::io::_eprint(
                                    format_args!(
                                        "{0}warn(via::ws): {1}\n",
                                        " ".repeat(2),
                                        &error,
                                    ),
                                );
                            };
                        }
                        this.reconnect();
                        context.waker().wake_by_ref();
                        Poll::Pending
                    }
                }
            }
        }
    }
    mod upgrade {
        use http::{HeaderMap, StatusCode, header};
        use hyper::upgrade::OnUpgrade;
        use std::future::Future;
        use std::sync::Arc;
        use tokio_tungstenite::WebSocketStream;
        use super::Channel;
        use super::io::UpgradedIo;
        use super::run::RunTask;
        use super::util::{Base64EncodedDigest, sha1};
        use crate::guard::header::{self as h, CaseSensitive, Contains, Header, Tag};
        use crate::guard::{self, And, Predicate};
        use crate::ws::error::UpgradeError;
        use crate::{
            BoxFuture, Error, Middleware, Next, Request, Response, ResultExt, raise,
        };
        const DEFAULT_FRAME_SIZE: usize = 16384;
        pub struct Ws<T> {
            listener: Arc<Listener<T>>,
            guard: And<
                (Header<CaseSensitive>, Header<Contains<Tag>>, Header<Contains<Tag>>),
            >,
        }
        pub(super) struct Listener<T> {
            pub(super) handle: T,
            config: WsConfig,
        }
        struct WsConfig {
            buffer_size: usize,
            max_frame_size: Option<usize>,
            max_message_size: Option<usize>,
        }
        #[automatically_derived]
        impl ::core::clone::Clone for WsConfig {
            #[inline]
            fn clone(&self) -> WsConfig {
                WsConfig {
                    buffer_size: ::core::clone::Clone::clone(&self.buffer_size),
                    max_frame_size: ::core::clone::Clone::clone(&self.max_frame_size),
                    max_message_size: ::core::clone::Clone::clone(&self.max_message_size),
                }
            }
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for WsConfig {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::debug_struct_field3_finish(
                    f,
                    "WsConfig",
                    "buffer_size",
                    &self.buffer_size,
                    "max_frame_size",
                    &self.max_frame_size,
                    "max_message_size",
                    &&self.max_message_size,
                )
            }
        }
        async fn handshake(
            config: &WsConfig,
            upgrade: OnUpgrade,
        ) -> Result<WebSocketStream<UpgradedIo>, Error> {
            use tungstenite::protocol::{Role, WebSocketConfig};
            let max_message_size = config.max_message_size;
            let mut config = WebSocketConfig::default()
                .accept_unmasked_frames(false)
                .read_buffer_size(config.buffer_size)
                .max_frame_size(config.max_frame_size)
                .max_message_size(max_message_size);
            if let Some(capacity) = max_message_size
                .and_then(|limit| limit.checked_mul(2))
            {
                config = config.write_buffer_size(capacity);
            }
            let stream = WebSocketStream::from_raw_socket(
                    UpgradedIo::new(upgrade.await?),
                    Role::Server,
                    Some(config),
                )
                .await;
            Ok(stream)
        }
        #[inline(always)]
        fn configure<T>(listener: &mut Arc<Listener<T>>) -> &mut WsConfig {
            Arc::get_mut(listener)
                .map(|listener| &mut listener.config)
                .expect("cannot be configure ws while the app is running.")
        }
        impl<T> Ws<T> {
            pub(super) fn new(listener: T) -> Self {
                Self {
                    listener: Arc::new(Listener {
                        handle: listener,
                        config: WsConfig::default(),
                    }),
                    guard: guard::and((
                        guard::header(
                            header::SEC_WEBSOCKET_VERSION,
                            h::case_sensitive(b"13"),
                        ),
                        guard::header(
                            header::CONNECTION,
                            h::contains(h::tag(b"upgrade")),
                        ),
                        guard::header(header::UPGRADE, h::contains(h::tag(b"websocket"))),
                    )),
                }
            }
            /// The amount of memory to pre-allocate in bytes for buffered reads.
            ///
            /// **Default:** `16 KB`
            ///
            pub fn buffer_size(mut self, capacity: usize) -> Self {
                configure(&mut self.listener).buffer_size = capacity;
                self
            }
            /// The maximum size of a single incoming message frame.
            ///
            /// A `None` value indicates no frame size limit.
            ///
            /// **Default:** `16 KB`
            ///
            pub fn max_frame_size(mut self, limit: Option<usize>) -> Self {
                configure(&mut self.listener).max_frame_size = limit;
                self
            }
            /// The maximum message size in bytes.
            ///
            /// **Default:** `16 KB`
            ///
            pub fn max_message_size(mut self, limit: Option<usize>) -> Self {
                configure(&mut self.listener).max_message_size = limit;
                self
            }
        }
        impl<T> Ws<T> {
            fn verify(
                &self,
                headers: &HeaderMap,
            ) -> Result<Base64EncodedDigest, UpgradeError> {
                self.guard.cmp(headers)?;
                let key = headers
                    .get(header::SEC_WEBSOCKET_KEY)
                    .ok_or(UpgradeError::MissingAcceptKey)?
                    .as_bytes();
                sha1(key)
            }
        }
        impl<T, App, Await> Middleware<App> for Ws<T>
        where
            T: Fn(Channel, super::Request<App>) -> Await + Send + Sync + 'static,
            App: Send + Sync + 'static,
            Await: Future<Output = super::Result> + Send + 'static,
        {
            fn call(&self, mut request: Request<App>, _: Next<App>) -> BoxFuture {
                let listener = Arc::clone(&self.listener);
                let Some(upgrade) = request.extensions_mut().remove::<OnUpgrade>() else {
                    return Box::pin(async {
                        return Err(
                            crate::Error::other_with_status(
                                Box::new(UpgradeError::Unsupported),
                                crate::error::StatusCode::INTERNAL_SERVER_ERROR,
                            ),
                        );
                    });
                };
                let accept = match self.verify(request.headers()).or_bad_request() {
                    Ok(digest) => digest,
                    Err(error) => {
                        return Box::pin(async { Err(error) });
                    }
                };
                Box::pin(async move {
                    let request = super::Request::new(request);
                    tokio::spawn(async move {
                        let stream = handshake(&listener.config, upgrade).await?;
                        RunTask::new(listener, request, stream).await
                    });
                    Response::build()
                        .status(StatusCode::SWITCHING_PROTOCOLS)
                        .header(header::CONNECTION, "upgrade")
                        .header(header::SEC_WEBSOCKET_ACCEPT, accept.as_str())
                        .header(header::UPGRADE, "websocket")
                        .finish()
                })
            }
        }
        impl Default for WsConfig {
            fn default() -> Self {
                Self {
                    buffer_size: DEFAULT_FRAME_SIZE,
                    max_frame_size: Some(DEFAULT_FRAME_SIZE),
                    max_message_size: Some(DEFAULT_FRAME_SIZE),
                }
            }
        }
    }
    mod util {
        mod future {
            use futures_task::noop_waker;
            use std::task::{Context, Poll};
            /// Call the provided closure with a `&mut Context` that uses a noop waker.
            pub fn poll_immediate_no_wake<T>(
                with: impl FnOnce(&mut Context) -> Poll<T>,
            ) -> Poll<T> {
                let waker = noop_waker();
                let mut cx = Context::from_waker(&waker);
                with(&mut cx)
            }
        }
        mod sha1 {
            use base64::engine::{Engine, general_purpose::STANDARD as base64};
            use ring::digest::{Context, SHA1_FOR_LEGACY_USE_ONLY};
            use crate::ws::error::UpgradeError;
            const WS_ACCEPT_GUID: &[u8] = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
            pub struct Base64EncodedDigest([u8; 28]);
            pub fn sha1(input: &[u8]) -> Result<Base64EncodedDigest, UpgradeError> {
                if !input.is_ascii() {
                    return Err(UpgradeError::InvalidAcceptEncoding);
                }
                let mut hasher = Context::new(&SHA1_FOR_LEGACY_USE_ONLY);
                let mut buf = [0; 28];
                hasher.update(input);
                hasher.update(WS_ACCEPT_GUID);
                if base64.encode_slice(hasher.finish(), &mut buf).is_ok() {
                    Ok(Base64EncodedDigest(buf))
                } else {
                    Err(UpgradeError::EncoderError)
                }
            }
            impl Base64EncodedDigest {
                #[inline(always)]
                pub fn as_str(&self) -> &str {
                    unsafe { str::from_utf8_unchecked(&self.0) }
                }
            }
        }
        pub use future::poll_immediate_no_wake;
        pub use sha1::{Base64EncodedDigest, sha1};
    }
    pub use channel::*;
    pub use error::{Result, ResultExt};
    pub use request::Request;
    pub use upgrade::Ws;
    /// Upgrade the connection to a web socket.
    ///
    /// *Note:*
    ///
    /// In order to guarantee progress of the receive loop of your web socket
    /// listener, you must await at least one future in the body of the loop.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::process::ExitCode;
    /// use via::ws::{self, Channel, Message};
    /// use via::{Error, Server};
    ///
    /// async fn echo(mut channel: Channel, _: ws::Request) -> ws::Result {
    ///     while let Some(message) = channel.recv().await {
    ///         if message.is_close() {
    ///             println!("info: close requested by client");
    ///             break;
    ///         }
    ///
    ///         if message.is_binary() || message.is_text() {
    ///             channel.send(message).await?;
    ///         } else if cfg!(debug_assertions) {
    ///             println!("warn: ignoring message {:?}", message);
    ///         }
    ///     }
    ///
    ///     Ok(())
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<ExitCode, Error> {
    ///     let mut app = via::app(());
    ///
    ///     // GET /echo ~> web socket upgrade.
    ///     app.route("/echo").to(via::ws(echo));
    ///
    ///     Server::new(app).listen(("127.0.0.1", 8080)).await
    /// }
    ///```
    ///
    pub fn ws<T, App, Await>(listener: T) -> Ws<T>
    where
        T: Fn(Channel, Request<App>) -> Await,
        Await: Future<Output = Result> + Send,
    {
        Ws::new(listener)
    }
}
mod app {
    mod service {
        use hyper::body::Incoming;
        use hyper::service::Service;
        use std::collections::VecDeque;
        use std::convert::Infallible;
        use std::pin::Pin;
        use std::sync::Arc;
        use std::task::{Context, Poll};
        use crate::request::{Envelope, Request, RequestBody};
        use crate::response::{Response, ResponseBody};
        use crate::server::ServerConfig;
        use crate::{BoxFuture, Next, Via, raise};
        const MAX_URI_PATH_LEN: usize = 8092;
        pub struct FutureResponse(BoxFuture);
        pub struct ServiceAdapter<App> {
            service: Arc<ViaService<App>>,
        }
        struct ViaService<App> {
            config: Box<ServerConfig>,
            via: Via<App>,
        }
        impl FutureResponse {
            fn max_path_len_exceeded() -> Self {
                Self(
                    Box::pin(async {
                        return Err(
                            crate::error::deny(
                                crate::error::StatusCode::URI_TOO_LONG,
                                "path exceeds the maximum allowed length of 8 KB",
                            ),
                        );
                    }),
                )
            }
        }
        impl Future for FutureResponse {
            type Output = Result<http::Response<ResponseBody>, Infallible>;
            fn poll(
                mut self: Pin<&mut Self>,
                context: &mut Context<'_>,
            ) -> Poll<Self::Output> {
                self.0
                    .as_mut()
                    .poll(context)
                    .map(|result| Ok(result.unwrap_or_else(Response::from).into()))
            }
        }
        impl<App> ServiceAdapter<App> {
            pub(crate) fn new(config: ServerConfig, via: Via<App>) -> Self {
                let config = Box::new(config);
                Self {
                    service: Arc::new(ViaService { config, via }),
                }
            }
            pub(crate) fn config(&self) -> &ServerConfig {
                &self.service.config
            }
        }
        impl<App> Clone for ServiceAdapter<App> {
            #[inline]
            fn clone(&self) -> Self {
                Self {
                    service: Arc::clone(&self.service),
                }
            }
        }
        impl<App> Service<http::Request<Incoming>> for ServiceAdapter<App> {
            type Error = Infallible;
            type Future = FutureResponse;
            type Response = http::Response<ResponseBody>;
            fn call(&self, request: http::Request<Incoming>) -> Self::Future {
                self.service.call(request)
            }
        }
        impl<App> Service<http::Request<Incoming>> for ViaService<App> {
            type Error = Infallible;
            type Future = FutureResponse;
            type Response = http::Response<ResponseBody>;
            fn call(&self, request: http::Request<Incoming>) -> Self::Future {
                let path = request.uri().path();
                if path.len() > MAX_URI_PATH_LEN {
                    return FutureResponse::max_path_len_exceeded();
                }
                let mut deque = VecDeque::with_capacity(18);
                let mut params = Vec::with_capacity(6);
                let frames = Vec::with_capacity(9);
                for (route, param) in self.via.router().traverse(path) {
                    deque.extend(route);
                    params.extend(param);
                }
                let app = self.via.app().clone();
                let request = {
                    let (parts, body) = request.into_parts();
                    let envelope = Envelope::new(parts, params);
                    let body = RequestBody::new(
                        self.config.max_request_size(),
                        body,
                        frames,
                    );
                    Request::new(envelope, body, app)
                };
                FutureResponse(Next::new(deque).call(request))
            }
        }
    }
    mod shared {
        use std::fmt::{self, Debug, Formatter};
        use std::ops::Deref;
        use std::sync::Arc;
        /// A thread-safe, reference-counting pointer to the application.
        ///
        /// An application is a user-defined struct that bundles together singleton
        /// resources whose lifetime matches that of the process in which it is created.
        ///
        /// `Shared` wraps an application and provides per-request ownership of the
        /// container. This allows resources to flow through async code without creating
        /// dangling borrows or introducing implicit lifetimes.
        ///
        /// Cloning a `Shared<App>` is inexpensive: it performs an atomic increment when
        /// cloned and an atomic decrement when dropped. When a client request is
        /// received, the `Shared` wrapper is cloned and ownership of the clone is
        /// transferred to the request.
        ///
        /// # Safe Access
        ///
        /// Async functions are compiled into state machines that may be suspended across
        /// `.await` points. Any borrow that outlives the data it references becomes
        /// invalid when the future is resumed, violating Rust’s safety guarantees.
        ///
        /// For many ["safe" (read-only)](http::Method::is_safe) requests, the application
        /// can be borrowed directly from the request without cloning or taking ownership
        /// of the underlying `Shared<App>`.
        ///
        /// ### Example
        ///
        /// ```
        /// use serde::{Deserialize, Serialize};
        /// use tokio::io::{self, AsyncWriteExt, Sink};
        /// use tokio::sync::{Mutex, RwLock};
        /// use via::{Next, Payload, Request, Response};
        ///
        /// /// An imaginary analytics service.
        /// struct Telemetry(Mutex<Sink>);
        ///
        /// /// Our billion dollar application.
        /// struct Unicorn {
        ///     telemetry: Telemetry,
        ///     users: RwLock<Vec<User>>,
        /// }
        ///
        /// #[derive(Clone, Deserialize)]
        /// struct NewUser {
        ///     email: String,
        ///     username: String,
        /// }
        ///
        /// #[derive(Clone, Serialize)]
        /// struct User {
        ///     id: i64,
        ///     email: String,
        ///     username: String,
        /// }
        ///
        /// impl Telemetry {
        ///     async fn report(&self, message: String) -> io::Result<()> {
        ///         let mut guard = self.0.lock().await;
        ///
        ///         guard.write_all(message.as_bytes()).await?;
        ///         guard.flush().await
        ///     }
        /// }
        ///
        /// impl From<NewUser> for User {
        ///     fn from(new_user: NewUser) -> Self {
        ///         Self {
        ///             id: 1235812,
        ///             email: new_user.email,
        ///             username: new_user.username,
        ///         }
        ///     }
        /// }
        ///
        /// async fn find_user(request: Request<Unicorn>, _: Next<Unicorn>) -> via::Result {
        ///     // Parse an i64 from the :user-id parameter in the request URI.
        ///     let id = request.param("user-id").parse::<i64>()?;
        ///
        ///     let user = {
        ///         // Acquire a read lock on the list of users.
        ///         let guard = request.app().users.read().await;
        ///
        ///         // Find the user with id = :id and clone it to keep contention low.
        ///         guard.iter().find(|user| id == user.id).cloned()
        ///     };
        ///
        ///     Response::build()
        ///         .status(user.is_some().then_some(200).unwrap_or(404))
        ///         .json(&user)
        /// }
        /// ```
        ///
        /// ## Handling Mutations
        ///
        /// For non-idempotent HTTP requests (e.g., DELETE, PATCH, POST), it is often
        /// necessary to consume the request in order to read the message body.
        ///
        /// In these cases, ownership of the `Shared<App>` is returned to the caller.
        /// This commonly occurs when a mutation requires acquiring a database
        /// connection or persisting state derived from the request body.
        ///
        /// This access pattern is safe, but any clone of `Shared<App>` that escapes the
        /// request future extends the lifetime of the application container and should
        /// be treated as an intentional design choice.
        ///
        /// ### Example
        ///
        /// ```
        /// # use serde::{Deserialize, Serialize};
        /// # use tokio::io::Sink;
        /// # use tokio::sync::{Mutex, RwLock};
        /// # use via::{Next, Payload, Request, Response};
        /// #
        /// # /// An imaginary analytics service.
        /// # struct Telemetry(Mutex<Sink>);
        /// #
        /// # /// Our billion dollar application.
        /// # struct Unicorn {
        /// #     telemetry: Telemetry,
        /// #     users: RwLock<Vec<User>>,
        /// # }
        /// #
        /// # #[derive(Clone, Deserialize)]
        /// # struct NewUser {
        /// #     email: String,
        /// #     username: String,
        /// # }
        /// #
        /// # #[derive(Clone, Serialize)]
        /// # struct User {
        /// #     id: i64,
        /// #     email: String,
        /// #     username: String,
        /// # }
        /// #
        /// # impl From<NewUser> for User {
        /// #     fn from(new_user: NewUser) -> Self {
        /// #         Self {
        /// #             id: 1235812,
        /// #             email: new_user.email,
        /// #             username: new_user.username,
        /// #         }
        /// #     }
        /// # }
        /// #
        /// async fn create_user(request: Request<Unicorn>, _: Next<Unicorn>) -> via::Result {
        ///     let (future, app) = request.into_future();
        ///     //           ^^^
        ///     // Ownership of the application is transferred so it can be accessed
        ///     // after the request body future resolves.
        ///     //
        ///     // This is correct so long as `app` is dropped before the function
        ///     // returns.
        ///
        ///     // Deserialize a NewUser struct from the JSON request body.
        ///     // Then, convert it to a User struct with a generated UUID.
        ///     let user: User = future.await?.json::<NewUser>()?.into();
        ///
        ///     // Acquire a write lock on the list of users and insert a clone.
        ///     // Dropping the lock as soon as we can beats the cost of memcpy.
        ///     app.users.write().await.push(user.clone());
        ///
        ///     // 201 Created!
        ///     Response::build().status(201).json(&user)
        /// }
        /// ```
        ///
        /// See: [`request.into_future()`] and [`request.into_parts()`].
        ///
        /// ## Detached Tasks and Atomic Contention
        ///
        /// `Shared<App>` relies on an atomic reference count to track ownership across
        /// threads. In typical request handling, the clone/drop rhythm closely follows
        /// the request lifecycle. This predictable cadence helps keep **atomic
        /// contention low**.
        ///
        /// Contention can be understood as waves:
        ///
        /// - Each request incrementing or decrementing the reference count forms a peak
        /// - If all requests align perfectly, peaks add together, increasing contention
        /// - In practice, requests are staggered in time, causing the peaks to partially
        ///   cancel and flatten
        ///
        /// ```text
        /// 'process: ──────────────────────────────────────────────────────────────────────────>
        ///            |                             |                              |
        ///        HTTP GET                          |                              |
        ///       app.clone()                        |                              |
        ///    incr strong_count                 HTTP GET                           |
        ///            |                        app.clone()                         |
        ///            |                     incr strong_count                  HTTP POST
        ///        List Users                        |                         app.clone()
        /// ┌──────────────────────┐                 |                      incr strong_count
        /// |   borrow req.app()   |        Web Socket Upgrade                      |
        /// |  acquire connection  |      ┌─────────────────────┐                   |
        /// |   respond with json  |      |     app_owned()     |              Create User
        /// └──────────────────────┘      |   spawn async task  |─┐     ┌──────────────────────┐
        ///     decr strong_count         | switching protocols | |     |   req.into_future()  |
        ///            |                  └─────────────────────┘ |     |     database trx     |
        ///            |                     decr strong_count    |     |       respond        |
        ///            |                             |            |     └──────────────────────┘
        ///            |                             |            |        decr strong_count
        ///            |                             |            |                 |
        ///            |                             |            └─>┌──────────────┐
        ///            |                             |               |  web socket  |
        ///            |                             |               └──────────────┘
        ///            |                             |               decr strong_count
        ///            |                             |                              |
        /// ┌──────────|─────────────────────────────|──────────────────────────────|───────────┐
        /// | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | |
        /// └──────────|─────────────────────────────|──────────────────────────────|───────────┘
        ///            |                             |                              |
        ///       uncontended                   uncontended                     contended
        /// ```
        ///
        /// Detached tasks disrupt this rhythm:
        ///
        /// - Their increments and decrements occur out of phase with the request
        ///   lifecycle
        /// - This can temporarily spike contention and extend resource lifetimes beyond
        ///   the request
        ///
        /// Keeping `Shared<App>` clones phase-aligned with the request lifecycle
        /// minimizes atomic contention and keeps resource lifetimes predictable. When
        /// the Arc is dropped deterministically as the middleware future resolves,
        /// leaks and unintended retention become significantly easier to detect.
        ///
        /// #### Timing and Side-Channel Awareness
        ///
        /// Each clone and drop of `Shared<App>` performs an atomic operation. When these
        /// operations occur out of phase with normal request handling (for example, in
        /// detached background tasks), they can introduce observable timing differences.
        ///
        /// In high-assurance systems, such differences may:
        ///
        /// - Act as unintended signals to an attacker
        /// - Reveal the presence of privileged handlers (e.g., [web socket upgrades])
        /// - Correlate background activity with specific request types
        ///
        /// In these cases, preserving a uniform request rhythm may be more valuable than
        /// minimizing contention. These tradeoffs should be made deliberately and
        /// documented, as they exchange throughput and modularity for reduced
        /// observability.
        ///
        /// ### Example
        ///
        /// ```
        /// # use serde::{Deserialize, Serialize};
        /// # use tokio::io::{self, AsyncWriteExt, Sink};
        /// # use tokio::sync::{Mutex, RwLock};
        /// # use via::{Next, Request, Response};
        /// #
        /// # /// An imaginary analytics service.
        /// # struct Telemetry(Mutex<Sink>);
        /// #
        /// # /// Our billion dollar application.
        /// # struct Unicorn {
        /// #     telemetry: Telemetry,
        /// #     users: RwLock<Vec<User>>,
        /// # }
        /// #
        /// # #[derive(Clone, Serialize)]
        /// # struct User {
        /// #     id: i64,
        /// #     email: String,
        /// #     username: String,
        /// # }
        /// #
        /// # impl Telemetry {
        /// #     async fn report(&self, message: String) -> io::Result<()> {
        /// #         let mut guard = self.0.lock().await;
        /// #
        /// #         guard.write_all(message.as_bytes()).await?;
        /// #         guard.flush().await
        /// #     }
        /// # }
        /// #
        /// async fn destroy_user(request: Request<Unicorn>, _: Next<Unicorn>) -> via::Result {
        ///     // Parse an i64 from the :user-id parameter in the request URI.
        ///     let id = request.param("user-id").parse::<i64>()?;
        ///
        ///     // This example favors anonymity over performance. Therefore, we clone
        ///     // app as early as we can.
        ///     //
        ///     // Any earlier and Uuid parse errors could become a DoS vector that
        ///     // rapidly clones app out-of-phase with other HTTP requests. Doing so
        ///     // would amplify contention peaks rather than cancelling them.
        ///     let app = request.app_owned();
        ///
        ///     let user = {
        ///         // Acquire a write lock on the list of users.
        ///         let mut guard = app.users.write().await;
        ///
        ///         // Find the user with id = :id and remove it from the list of users.
        ///         guard.pop_if(|user| &id == &user.id)
        ///     };
        ///
        ///     // The status code of the response is the only dependency of user other
        ///     // than the telemetry task. Compute it early so user can be moved into
        ///     // the spawned task.
        ///     let status = user.is_some().then_some(204).unwrap_or(404);
        ///
        ///     // Spawn a task that owns of all of its dependencies.
        ///     //
        ///     // *Note:*
        ///     //
        ///     // The condition that determines whether or not we should report to the
        ///     // destroy op must be computed inside of the spawned task. This avoids
        ///     // a signal (task spawn) that can be used to determine the success of
        ///     // the write op outside of the HTTP transaction.
        ///     tokio::spawn(async move {
        ///         if let Some(User { id, .. }) = user {
        ///             let message = format!("delete: resource = users, id = {}", id);
        ///             let _ = app.telemetry.report(message).await.inspect_err(|error| {
        ///                 if cfg!(debug_assertions) {
        ///                     eprintln!("error(telemetry): {}", error);
        ///                 }
        ///             });
        ///         }
        ///     });
        ///
        ///     Response::build().status(status).finish()
        /// }
        /// ```
        ///
        /// [`request.into_future()`]: crate::Request::into_future
        /// [`request.into_parts()`]: crate::Request::into_parts
        /// [web socket upgrades]: ../src/via/ws/upgrade.rs.html#256-262
        ///
        pub struct Shared<App>(Arc<App>);
        impl<App> Shared<App> {
            pub(super) fn new(value: App) -> Self {
                Self(Arc::new(value))
            }
        }
        impl<App> AsRef<App> for Shared<App> {
            #[inline]
            fn as_ref(&self) -> &App {
                &self.0
            }
        }
        impl<App> Clone for Shared<App> {
            #[inline]
            fn clone(&self) -> Self {
                Self(Arc::clone(&self.0))
            }
        }
        impl<App> Debug for Shared<App> {
            fn fmt(&self, f: &mut Formatter) -> fmt::Result {
                f.debug_struct("Shared").finish()
            }
        }
        impl<App> Deref for Shared<App> {
            type Target = App;
            #[inline]
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
    }
    pub(crate) use service::ServiceAdapter;
    pub use shared::Shared;
    use delegate::delegate;
    use crate::middleware::Middleware;
    use crate::router::{Route, Router};
    /// Configure routes and define shared global state.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::process::ExitCode;
    /// use via::{Error, Next, Request, Server, Shared};
    ///
    /// /// A mock database pool.
    /// #[derive(Debug)]
    /// struct DatabasePool {
    ///     url: String,
    /// }
    ///
    /// /// Shared global state. Named after our application.
    /// struct Unicorn {
    ///     pool: DatabasePool,
    /// }
    ///
    /// impl Unicorn {
    ///     fn pool(&self) -> &DatabasePool {
    ///         &self.pool
    ///     }
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<ExitCode, Error> {
    ///     // Pass our shared state struct containing a database pool to the App
    ///     // constructor so it can be used to serve each request.
    ///     let mut app = via::app(Unicorn {
    ///         pool: DatabasePool {
    ///             url: std::env::var("DATABASE_URL")?,
    ///         },
    ///     });
    ///
    ///     // We can access our database in middleware with `request.app()`.
    ///     app.middleware(async |request: Request<Unicorn>, next: Next<Unicorn>| {
    ///         // Print the debug output of our mock database pool to stdout.
    ///         println!("{:?}", request.app().pool());
    ///
    ///         // Delegate to the next middleware to get a response.
    ///         next.call(request).await
    ///     });
    ///
    ///     // Start serving our application from http://localhost:8080/.
    ///     Server::new(app).listen(("127.0.0.1", 8080)).await
    /// }
    /// ```
    ///
    pub struct Via<App> {
        app: Shared<App>,
        router: Router<App>,
    }
    /// Create a new app with the provided state argument.
    ///
    /// # Example
    ///
    /// ```
    /// # struct DatabasePool { url: String }
    /// # struct Unicorn { pool: DatabasePool }
    /// #
    /// let mut app = via::app(Unicorn {
    ///     pool: DatabasePool {
    ///         url: "postgres://unicorn@localhost/unicorn".to_owned(),
    ///     },
    /// });
    /// ```
    ///
    pub fn app<App>(app: App) -> Via<App> {
        Via {
            app: Shared::new(app),
            router: Router::new(),
        }
    }
    impl<App> Via<App> {
        /// Append the provided middleware to applications call stack.
        ///
        /// Middleware attached with this method runs for every request.
        ///
        /// See also the usage example in [`Route::middleware`].
        #[inline]
        pub fn middleware<T>(&mut self, middleware: T)
        where
            T: Middleware<App> + 'static,
        {
            self.router.route("/").middleware::<T>(middleware);
        }
        /// Returns a new route as a child of the root path `/`.
        ///
        /// See also the usage example in [`Route::route`].
        #[inline]
        pub fn route(&mut self, path: &'static str) -> Route<'_, App> {
            self.router.route(path)
        }
    }
    impl<App> Via<App> {
        pub(crate) fn app(&self) -> &Shared<App> {
            &self.app
        }
        pub(crate) fn router(&self) -> &Router<App> {
            &self.router
        }
    }
}
mod cookies {
    use bytes::Bytes;
    use cookie::{Cookie, SplitCookies};
    use http::header::{COOKIE, SET_COOKIE};
    use http::{HeaderValue, header};
    use std::collections::HashSet;
    use std::fmt::{self, Display, Formatter};
    use std::mem;
    use crate::util::UriEncoding;
    use crate::{BoxFuture, Middleware, Next, Request};
    /// An error occurred while writing a Set-Cookie header to a response.
    ///
    struct SetCookieError;
    #[automatically_derived]
    impl ::core::fmt::Debug for SetCookieError {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            ::core::fmt::Formatter::write_str(f, "SetCookieError")
        }
    }
    /// Parse and manage the specified request and response cookies.
    ///
    /// A bidirectional middleware that parses the cookie header of an incoming
    /// request and extends the request's cookie jar with the extracted cookies,
    /// then calls `next` to obtain a response and serializes any modified cookies
    /// into `Set-Cookie` headers.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use cookie::{Cookie, SameSite};
    /// use std::process::ExitCode;
    /// use std::time::Duration;
    /// use via::{Error, Next, Request, Response, ResultExt, Server, cookies};
    ///
    /// async fn greet(request: Request, _: Next) -> via::Result {
    ///     // `should_set_name` indicates whether "name" was sourced from the
    ///     // request URI. When false, the "name" cookie should not be modified.
    ///     //
    ///     // `name` is a Cow that contains either the percent-decoded value of
    ///     // the "name" cookie or the percent-decoded value of the "name"
    ///     // parameter in the request URI.
    ///     let (should_set_name, name) = match request.cookies().get("name") {
    ///         Some(cookie) => (false, cookie.value().into()),
    ///         None => (true, request.param("name").percent_decode().or_bad_request()?),
    ///     };
    ///
    ///     // Build the greeting response using a reference to name.
    ///     let mut response = Response::build().text(format!("Hello, {}!", name.as_ref()))?;
    ///
    ///     // If "name" came from the request uri, set the "name" cookie.
    ///     if should_set_name {
    ///         response.cookies_mut().add(
    ///             Cookie::build(("name", name.into_owned()))
    ///                 .http_only(true)
    ///                 .max_age(Duration::from_hours(1).try_into()?)
    ///                 .path("/")
    ///                 .same_site(SameSite::Strict)
    ///                 .secure(true),
    ///         );
    ///     }
    ///
    ///     Ok(response)
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<ExitCode, Error> {
    ///     let mut app = via::app(());
    ///
    ///     // Provide cookie support for downstream middleware.
    ///     app.middleware(via::cookies(["name"]).decode());
    ///
    ///     // Respond with a greeting when a user visits /hello/:name.
    ///     app.route("/hello/:name").to(via::get(greet));
    ///
    ///     // Start serving our application from http://localhost:8080/.
    ///     Server::new(app).listen(("127.0.0.1", 8080)).await
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// The Cookies middleware responds with a `500` error if any of the following
    /// conditions are met:
    ///
    /// - A Set-Cookie header cannot be constructed
    /// - The maximum capacity of the response header map is exceeded
    ///
    /// # Security
    ///
    /// In production, we recommend using either a
    /// [`SignedJar`](https://docs.rs/cookie/latest/cookie/struct.SignedJar.html)
    /// or
    /// [`PrivateJar`](https://docs.rs/cookie/latest/cookie/struct.PrivateJar.html)
    /// to store security sensitive cookies.
    ///
    /// A _signed jar_ signs all cookies added to it and verifies cookies retrieved
    /// from it, preventing clients from tampering with or fabricating cookie data.
    /// A _private jar_ both signs and encrypts cookies, providing all the
    /// guarantees of a signed jar while also ensuring confidentiality.
    ///
    /// ## Best Practices
    ///
    /// As a best practice, in order to mitigate the vast majority of security
    /// related concerns of shared state with a client via cookies–we recommend
    /// setting `HttpOnly`, `Max-Age`, `SameSite=Strict`, and `Secure` for every
    /// cookie used by your application.
    ///
    /// - `HttpOnly`<br>
    ///   Prevents client-side scripts from accessing the cookie, mitigating cross-
    ///   site scripting (XSS) attacks. This should be enabled for any cookie that
    ///   does not need to be accessed directly from JavaScript. Requests made from
    ///   JavaScript using the Fetch API with `credentials: "include"` or
    ///   `"same-origin"` automatically include all relevant cookies for the
    ///   request's origin, including those marked as `HttpOnly`.
    ///
    /// - `Max-Age`<br>
    ///   Limits how long the browser will store and send the cookie. This reduces
    ///   the window in which a leaked or stolen cookie can be used, and helps
    ///   prevent session accumulation on the client.
    ///
    /// - `SameSite=Strict`<br>
    ///   Restricts cookies to same-site requests, mitigating CSRF attacks. If the
    ///   cookie does not need to be shared cross-site, this setting practically
    ///   eliminates CSRF risk in modern browsers. However, it prevents
    ///   authentication flows that involve redirects from external identity
    ///   providers (OAuth, SAML, etc.).
    ///
    /// - `Secure`<br>
    ///   Instructs the client to only include the cookie in requests made using
    ///   the `https:` scheme or to `localhost`.
    ///
    /// ```no_run
    /// use cookie::{Cookie, SameSite};
    /// use http::StatusCode;
    /// use serde::Deserialize;
    /// use std::process::ExitCode;
    /// use std::time::Duration;
    /// use via::{Error, Next, Payload, Request, Response, Server, cookies};
    ///
    /// #[derive(Deserialize)]
    /// struct Login {
    ///     username: String,
    ///     password: String,
    /// }
    ///
    /// async fn login(request: Request, _: Next) -> via::Result {
    ///     let (body, app) = request.into_future();
    ///     let params = body.await?.json::<Login>()?;
    ///
    ///     // Insert username and password verification here...
    ///     // For now, we'll just assert that the password is not empty.
    ///     if params.password.is_empty() {
    ///         via::raise!(401, message = "Invalid username or password.");
    ///     }
    ///
    ///     // Generate a response with no content.
    ///     //
    ///     // If we were verifying that a user with the provided username and
    ///     // password exists in a database table, we'd probably respond with the
    ///     // matching row as JSON.
    ///     let mut response = Response::build().status(204).finish()?;
    ///
    ///     // Add our session cookie that contains the username of the active user
    ///     // to our signed cookie jar. The value of the cookie will be signed
    ///     // and encrypted before it is included as a set-cookie header.
    ///     response.cookies_mut().add(
    ///         Cookie::build(("via-session", params.username))
    ///             .http_only(true)
    ///             .max_age(Duration::from_hours(1).try_into()?)
    ///             .path("/")
    ///             .same_site(SameSite::Strict)
    ///             .secure(true),
    ///     );
    ///
    ///     Ok(response)
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<ExitCode, Error> {
    ///     let mut app = via::app(());
    ///
    ///     // Unencoded cookie support.
    ///     app.middleware(via::cookies(["session"]));
    ///
    ///     // Add our login route to our application.
    ///     app.route("/auth/login").to(via::post(login));
    ///
    ///     // Start serving our application from http://localhost:8080/.
    ///     Server::new(app).listen(("127.0.0.1", 8080)).await
    /// }
    /// ```
    ///
    pub struct Cookies {
        encoding: UriEncoding,
        allow: HashSet<String>,
    }
    /// Parse and manage the specified request and response cookies.
    ///
    /// The default behavior of the Cookies middleware is to ignore all cookies
    /// unless they are specified by name in the provided allow list.
    ///
    /// This prevents irrelevant cookies from becoming a DoS vector by keeping
    /// the length of the request and response cookie jars bounded.
    ///
    /// # Example
    ///
    /// ```
    /// # let mut app = via::app(());
    /// app.middleware(via::cookies(["session"]));
    /// ```
    ///
    pub fn cookies<I>(allow: I) -> Cookies
    where
        I: IntoIterator,
        I::Item: AsRef<str> + 'static,
    {
        allow.into_iter().fold(Cookies::new(), Cookies::allow)
    }
    #[inline(always)]
    fn split_parse<'a>(encoding: &UriEncoding, input: &'a str) -> SplitCookies<'a> {
        if let UriEncoding::Percent = *encoding {
            Cookie::split_parse_encoded(input)
        } else {
            Cookie::split_parse(input)
        }
    }
    impl Cookies {
        /// Specify that cookies should be percent-decoded when parsed and percent-
        /// encoded when serialized as a Set-Cookie header.
        ///
        /// # Example
        ///
        /// ```
        /// # let mut app = via::app(());
        /// app.middleware(via::cookies(["session"]).decode());
        /// ```
        ///
        pub fn decode(mut self) -> Self {
            self.encoding = UriEncoding::Percent;
            self
        }
    }
    impl Cookies {
        fn new() -> Self {
            Self {
                encoding: UriEncoding::Unencoded,
                allow: HashSet::new(),
            }
        }
        fn allow(mut self, name: impl AsRef<str>) -> Self {
            self.allow.insert(name.as_ref().to_owned());
            self
        }
        fn parse(&self, input: &str) -> impl Iterator<Item = Cookie<'static>> {
            split_parse(&self.encoding, input)
                .filter_map(|result| {
                    let shared = match result {
                        Ok(cookie) => cookie,
                        Err(error) => {
                            if true {
                                {
                                    ::std::io::_eprint(
                                        format_args!("warn(cookies): {0}\n", error),
                                    );
                                };
                            }
                            return None;
                        }
                    };
                    self.allow.contains(shared.name()).then(|| shared.into_owned())
                })
        }
    }
    impl<App> Middleware<App> for Cookies {
        fn call(&self, mut request: Request<App>, next: Next<App>) -> BoxFuture {
            let existing = request
                .headers()
                .get(COOKIE)
                .and_then(|header| {
                    let input = header.to_str().ok()?;
                    Some(self.parse(input).collect::<Vec<_>>())
                });
            if let Some(cookies) = existing.as_deref() {
                cookies
                    .iter()
                    .cloned()
                    .for_each(|cookie| {
                        request.cookies_mut().add_original(cookie);
                    });
            }
            let encoding = self.encoding;
            let future = next.call(request);
            Box::pin(async move {
                let mut response = future.await?;
                let mut cookies = mem::take(response.cookies_mut());
                if let Some(original) = existing {
                    for cookie in original {
                        cookies.add_original(cookie);
                    }
                }
                cookies
                    .delta()
                    .try_for_each(|cookie| {
                        let value = HeaderValue::from_maybe_shared::<
                            Bytes,
                        >(
                            match encoding {
                                UriEncoding::Percent => cookie.encoded().to_string().into(),
                                UriEncoding::Unencoded => cookie.to_string().into(),
                            },
                        )?;
                        response.headers_mut().try_append(SET_COOKIE, value)?;
                        Ok::<_, SetCookieError>(())
                    })?;
                Ok(response)
            })
        }
    }
    impl std::error::Error for SetCookieError {}
    impl Display for SetCookieError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_fmt(
                format_args!(
                    "An error occurred while writing a Set-Cookie header to a response.",
                ),
            )
        }
    }
    impl From<header::MaxSizeReached> for SetCookieError {
        fn from(_: header::MaxSizeReached) -> Self {
            Self
        }
    }
    impl From<header::InvalidHeaderValue> for SetCookieError {
        fn from(_: header::InvalidHeaderValue) -> Self {
            Self
        }
    }
}
mod middleware {
    use std::future::Future;
    use std::pin::Pin;
    use super::next::Next;
    use crate::error::Error;
    use crate::request::Request;
    use crate::response::Response;
    /// An alias for the pin
    /// [`Box<dyn Future>`]
    /// returned by
    /// [middleware](Middleware::call).
    ///
    pub type BoxFuture<T = Result> = Pin<Box<dyn Future<Output = T> + Send>>;
    /// An alias for results that uses the [`Error`] struct defined in this crate.
    ///
    pub type Result<T = Response> = std::result::Result<T, Error>;
    pub trait Middleware<App>: Send + Sync {
        fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture;
    }
    impl<T, Await, App> Middleware<App> for T
    where
        T: Fn(Request<App>, Next<App>) -> Await + Send + Sync,
        Await: Future<Output = Result> + Send + 'static,
    {
        fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture {
            Box::pin(self(request, next))
        }
    }
}
mod next {
    use std::collections::VecDeque;
    use std::sync::Arc;
    use crate::middleware::{BoxFuture, Middleware};
    use crate::request::Request;
    /// A no-op middleware that simply calls the next middleware in the stack.
    ///
    /// `Continue` acts as a neutral element in middleware composition. It performs
    /// no work of its own and immediately forwards the request to `next`.
    ///
    /// Although it may appear trivial, `Continue` is a useful building block for
    /// implementing middleware combinators that provide custom branching logic
    /// where a concrete fallback is required.
    pub struct Continue;
    /// The next middleware in the logical call stack of a request.
    pub struct Next<App = ()> {
        deque: VecDeque<Arc<dyn Middleware<App>>>,
    }
    impl<App> Middleware<App> for Continue {
        fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture {
            next.call(request)
        }
    }
    impl<App> Next<App> {
        #[inline]
        pub(crate) fn new(deque: VecDeque<Arc<dyn Middleware<App>>>) -> Self {
            Self { deque }
        }
        /// Calls the next middleware in the logical call stack of the request.
        ///
        /// # Example
        ///
        /// ```
        /// use via::{Request, Next};
        ///
        /// async fn logger(request: Request, next: Next) -> via::Result {
        ///     let head = request.envelope();
        ///
        ///     println!("{} -> {}", head.method(), head.uri().path());
        ///     next.call(request).await.inspect(|response| {
        ///         println!("<- {}", response.status());
        ///     })
        /// }
        /// ```
        pub fn call(mut self, request: Request<App>) -> BoxFuture {
            match self.deque.pop_front() {
                Some(middleware) => middleware.call(request, self),
                None => {
                    Box::pin(async {
                        {
                            let status = crate::error::StatusCode::NOT_FOUND;
                            let message = status
                                .canonical_reason()
                                .unwrap_or_default()
                                .to_owned()
                                .to_ascii_lowercase() + ".";
                            return Err(crate::error::deny(status, message));
                        }
                    })
                }
            }
        }
    }
    impl<App> Drop for Next<App> {
        fn drop(&mut self) {}
    }
}
mod rest {}
mod server {
    //! Serve an [App](crate::App) over HTTP or HTTPS.
    //!
    mod accept {
        use hyper::server::conn::*;
        use hyper_util::rt::TokioTimer;
        use std::mem;
        use std::process::ExitCode;
        use std::sync::Arc;
        use tokio::io::{AsyncRead, AsyncWrite};
        use tokio::net::TcpListener;
        use tokio::sync::Semaphore;
        use tokio::task::{JoinSet, coop};
        use tokio::time::timeout;
        use super::cancel::Cancellation;
        use super::{IoWithPermit, tls};
        use crate::app::ServiceAdapter;
        use crate::error::ServerError;
        pub(super) async fn accept<App, TlsAcceptor>(
            acceptor: TlsAcceptor,
            listener: TcpListener,
            service: ServiceAdapter<App>,
        ) -> ExitCode
        where
            App: Send + Sync + 'static,
            ServerError: From<TlsAcceptor::Error>,
            TlsAcceptor: tls::Acceptor,
            TlsAcceptor::Io: Send + Unpin + 'static,
        {
            drop(acceptor);
            let semaphore = Arc::new(Semaphore::new(service.config().max_connections()));
            let mut connections = JoinSet::new();
            let cancellation = wait_for_ctrl_c();
            let exit_code = loop {
                let (io, _) = {
                    #[doc(hidden)]
                    mod __tokio_select_util {
                        pub(super) enum Out<_0, _1> {
                            _0(_0),
                            _1(_1),
                            Disabled,
                        }
                        pub(super) type Mask = u8;
                    }
                    use ::tokio::macros::support::Pin;
                    const BRANCHES: u32 = 2;
                    let mut disabled: __tokio_select_util::Mask = Default::default();
                    if !true {
                        let mask: __tokio_select_util::Mask = 1 << 0;
                        disabled |= mask;
                    }
                    if !true {
                        let mask: __tokio_select_util::Mask = 1 << 1;
                        disabled |= mask;
                    }
                    let mut output = {
                        let futures_init = (listener.accept(), cancellation.wait());
                        let mut futures = (
                            ::tokio::macros::support::IntoFuture::into_future(
                                futures_init.0,
                            ),
                            ::tokio::macros::support::IntoFuture::into_future(
                                futures_init.1,
                            ),
                        );
                        let mut futures = &mut futures;
                        ::tokio::macros::support::poll_fn(|cx| {
                                match ::tokio::macros::support::poll_budget_available(cx) {
                                    ::core::task::Poll::Ready(t) => t,
                                    ::core::task::Poll::Pending => {
                                        return ::core::task::Poll::Pending;
                                    }
                                };
                                let mut is_pending = false;
                                let start = 0;
                                for i in 0..BRANCHES {
                                    let branch;
                                    #[allow(clippy::modulo_one)]
                                    {
                                        branch = (start + i) % BRANCHES;
                                    }
                                    match branch {
                                        #[allow(unreachable_code)]
                                        0 => {
                                            let mask = 1 << branch;
                                            if disabled & mask == mask {
                                                continue;
                                            }
                                            let (fut, ..) = &mut *futures;
                                            let mut fut = unsafe {
                                                ::tokio::macros::support::Pin::new_unchecked(fut)
                                            };
                                            let out = match ::tokio::macros::support::Future::poll(
                                                fut,
                                                cx,
                                            ) {
                                                ::tokio::macros::support::Poll::Ready(out) => out,
                                                ::tokio::macros::support::Poll::Pending => {
                                                    is_pending = true;
                                                    continue;
                                                }
                                            };
                                            disabled |= mask;
                                            #[allow(unused_variables)] #[allow(unused_mut)]
                                            match &out {
                                                result => {}
                                                _ => continue,
                                            }
                                            return ::tokio::macros::support::Poll::Ready(
                                                __tokio_select_util::Out::_0(out),
                                            );
                                        }
                                        #[allow(unreachable_code)]
                                        1 => {
                                            let mask = 1 << branch;
                                            if disabled & mask == mask {
                                                continue;
                                            }
                                            let (_, fut, ..) = &mut *futures;
                                            let mut fut = unsafe {
                                                ::tokio::macros::support::Pin::new_unchecked(fut)
                                            };
                                            let out = match ::tokio::macros::support::Future::poll(
                                                fut,
                                                cx,
                                            ) {
                                                ::tokio::macros::support::Poll::Ready(out) => out,
                                                ::tokio::macros::support::Poll::Pending => {
                                                    is_pending = true;
                                                    continue;
                                                }
                                            };
                                            disabled |= mask;
                                            #[allow(unused_variables)] #[allow(unused_mut)]
                                            match &out {
                                                _ => {}
                                                _ => continue,
                                            }
                                            return ::tokio::macros::support::Poll::Ready(
                                                __tokio_select_util::Out::_1(out),
                                            );
                                        }
                                        _ => {
                                            ::core::panicking::panic_fmt(
                                                format_args!(
                                                    "internal error: entered unreachable code: {0}",
                                                    format_args!(
                                                        "reaching this means there probably is an off by one bug",
                                                    ),
                                                ),
                                            );
                                        }
                                    }
                                }
                                if is_pending {
                                    ::tokio::macros::support::Poll::Pending
                                } else {
                                    ::tokio::macros::support::Poll::Ready(
                                        __tokio_select_util::Out::Disabled,
                                    )
                                }
                            })
                            .await
                    };
                    match output {
                        __tokio_select_util::Out::_0(result) => {
                            match result {
                                Ok(stream) => stream,
                                Err(error) => {
                                    if true {
                                        {
                                            ::std::io::_eprint(
                                                format_args!("error(accept): {0}\n", error),
                                            );
                                        }
                                    }
                                    let Some(12 | 23 | 24) = error.raw_os_error() else {
                                        continue;
                                    };
                                    break ExitCode::FAILURE;
                                }
                            }
                        }
                        __tokio_select_util::Out::_1(_) => {
                            break ExitCode::SUCCESS;
                        }
                        __tokio_select_util::Out::Disabled => {
                            ::core::panicking::panic_fmt(
                                format_args!(
                                    "all branches are disabled and there is no else branch",
                                ),
                            );
                        }
                        _ => {
                            ::core::panicking::panic_fmt(
                                format_args!(
                                    "internal error: entered unreachable code: {0}",
                                    format_args!("failed to match bind"),
                                ),
                            );
                        }
                    }
                };
                let Ok(permit) = semaphore.clone().try_acquire_owned() else {
                    continue;
                };
                let service = service.clone();
                let cancellation = cancellation.clone();
                connections
                    .spawn(async move {
                        let io = IoWithPermit::new(io, permit);
                        serve_http1_connection(io, service, cancellation).await
                    });
                if connections.len() >= 1024 {
                    let batch = mem::take(&mut connections);
                    tokio::spawn(drain_connections(false, batch));
                }
            };
            match timeout(
                    service.config().shutdown_timeout(),
                    drain_connections(true, connections),
                )
                .await
            {
                Ok(_) => exit_code,
                Err(_) => ExitCode::FAILURE,
            }
        }
        async fn drain_connections(
            immediate: bool,
            mut connections: JoinSet<Result<(), ServerError>>,
        ) {
            if true {
                {
                    ::std::io::_eprint(
                        format_args!(
                            "joining {0} inflight connections...\n",
                            connections.len(),
                        ),
                    );
                }
            }
            while let Some(result) = connections.join_next().await {
                match result {
                    Ok(Ok(_)) => {}
                    Err(error) => {
                        if true {
                            {
                                ::std::io::_eprint(
                                    format_args!("error(connection): {0}\n", error),
                                );
                            }
                        }
                    }
                    Ok(Err(error)) => {
                        if true {
                            {
                                ::std::io::_eprint(
                                    format_args!("error(service): {0}\n", error),
                                );
                            }
                        }
                    }
                }
                if !immediate {
                    coop::consume_budget().await;
                }
            }
        }
        async fn serve_http1_connection<App, Io>(
            io: IoWithPermit<Io>,
            service: ServiceAdapter<App>,
            cancellation: Cancellation,
        ) -> Result<(), ServerError>
        where
            App: Send + Sync + 'static,
            Io: AsyncRead + AsyncWrite + Send + Unpin + 'static,
        {
            let connection = http1::Builder::new()
                .allow_multiple_spaces_in_request_line_delimiters(false)
                .auto_date_header(true)
                .half_close(false)
                .ignore_invalid_headers(false)
                .keep_alive(service.config().keep_alive())
                .max_buf_size(service.config().max_buf_size())
                .pipeline_flush(false)
                .preserve_header_case(false)
                .timer(TokioTimer::new())
                .title_case_headers(false)
                .serve_connection(io, service)
                .with_upgrades();
            let mut connection = connection;
            #[allow(unused_mut)]
            let mut connection = unsafe {
                ::tokio::macros::support::Pin::new_unchecked(&mut connection)
            };
            {
                #[doc(hidden)]
                mod __tokio_select_util {
                    pub(super) enum Out<_0, _1> {
                        _0(_0),
                        _1(_1),
                        Disabled,
                    }
                    pub(super) type Mask = u8;
                }
                use ::tokio::macros::support::Pin;
                const BRANCHES: u32 = 2;
                let mut disabled: __tokio_select_util::Mask = Default::default();
                if !true {
                    let mask: __tokio_select_util::Mask = 1 << 0;
                    disabled |= mask;
                }
                if !true {
                    let mask: __tokio_select_util::Mask = 1 << 1;
                    disabled |= mask;
                }
                let mut output = {
                    let futures_init = (connection.as_mut(), cancellation.wait());
                    let mut futures = (
                        ::tokio::macros::support::IntoFuture::into_future(
                            futures_init.0,
                        ),
                        ::tokio::macros::support::IntoFuture::into_future(futures_init.1),
                    );
                    let mut futures = &mut futures;
                    ::tokio::macros::support::poll_fn(|cx| {
                            match ::tokio::macros::support::poll_budget_available(cx) {
                                ::core::task::Poll::Ready(t) => t,
                                ::core::task::Poll::Pending => {
                                    return ::core::task::Poll::Pending;
                                }
                            };
                            let mut is_pending = false;
                            let start = 0;
                            for i in 0..BRANCHES {
                                let branch;
                                #[allow(clippy::modulo_one)]
                                {
                                    branch = (start + i) % BRANCHES;
                                }
                                match branch {
                                    #[allow(unreachable_code)]
                                    0 => {
                                        let mask = 1 << branch;
                                        if disabled & mask == mask {
                                            continue;
                                        }
                                        let (fut, ..) = &mut *futures;
                                        let mut fut = unsafe {
                                            ::tokio::macros::support::Pin::new_unchecked(fut)
                                        };
                                        let out = match ::tokio::macros::support::Future::poll(
                                            fut,
                                            cx,
                                        ) {
                                            ::tokio::macros::support::Poll::Ready(out) => out,
                                            ::tokio::macros::support::Poll::Pending => {
                                                is_pending = true;
                                                continue;
                                            }
                                        };
                                        disabled |= mask;
                                        #[allow(unused_variables)] #[allow(unused_mut)]
                                        match &out {
                                            result => {}
                                            _ => continue,
                                        }
                                        return ::tokio::macros::support::Poll::Ready(
                                            __tokio_select_util::Out::_0(out),
                                        );
                                    }
                                    #[allow(unreachable_code)]
                                    1 => {
                                        let mask = 1 << branch;
                                        if disabled & mask == mask {
                                            continue;
                                        }
                                        let (_, fut, ..) = &mut *futures;
                                        let mut fut = unsafe {
                                            ::tokio::macros::support::Pin::new_unchecked(fut)
                                        };
                                        let out = match ::tokio::macros::support::Future::poll(
                                            fut,
                                            cx,
                                        ) {
                                            ::tokio::macros::support::Poll::Ready(out) => out,
                                            ::tokio::macros::support::Poll::Pending => {
                                                is_pending = true;
                                                continue;
                                            }
                                        };
                                        disabled |= mask;
                                        #[allow(unused_variables)] #[allow(unused_mut)]
                                        match &out {
                                            _ => {}
                                            _ => continue,
                                        }
                                        return ::tokio::macros::support::Poll::Ready(
                                            __tokio_select_util::Out::_1(out),
                                        );
                                    }
                                    _ => {
                                        ::core::panicking::panic_fmt(
                                            format_args!(
                                                "internal error: entered unreachable code: {0}",
                                                format_args!(
                                                    "reaching this means there probably is an off by one bug",
                                                ),
                                            ),
                                        );
                                    }
                                }
                            }
                            if is_pending {
                                ::tokio::macros::support::Poll::Pending
                            } else {
                                ::tokio::macros::support::Poll::Ready(
                                    __tokio_select_util::Out::Disabled,
                                )
                            }
                        })
                        .await
                };
                match output {
                    __tokio_select_util::Out::_0(result) => result?,
                    __tokio_select_util::Out::_1(_) => {
                        connection.as_mut().graceful_shutdown();
                        connection.await?;
                    }
                    __tokio_select_util::Out::Disabled => {
                        ::core::panicking::panic_fmt(
                            format_args!(
                                "all branches are disabled and there is no else branch",
                            ),
                        );
                    }
                    _ => {
                        ::core::panicking::panic_fmt(
                            format_args!(
                                "internal error: entered unreachable code: {0}",
                                format_args!("failed to match bind"),
                            ),
                        );
                    }
                }
            }
            Ok(())
        }
        fn wait_for_ctrl_c() -> Cancellation {
            let (cancellation, remote) = Cancellation::new();
            tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_err() {
                    {
                        ::std::io::_eprint(
                            format_args!("unable to register the \'ctrl-c\' signal.\n"),
                        );
                    };
                }
                remote.cancel();
            });
            cancellation
        }
    }
    mod cancel {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        use tokio::sync::Notify;
        pub struct Cancellation(Arc<Inner>);
        #[automatically_derived]
        impl ::core::clone::Clone for Cancellation {
            #[inline]
            fn clone(&self) -> Cancellation {
                Cancellation(::core::clone::Clone::clone(&self.0))
            }
        }
        pub struct Remote(Arc<Inner>);
        struct Inner {
            cancelled: AtomicBool,
            notify: Notify,
        }
        impl Cancellation {
            pub fn new() -> (Self, Remote) {
                let inner = Arc::new(Inner {
                    cancelled: AtomicBool::new(false),
                    notify: Notify::new(),
                });
                (Self(Arc::clone(&inner)), Remote(inner))
            }
            pub async fn wait(&self) {
                let inner = &*self.0;
                if !inner.cancelled.load(Ordering::Acquire) {
                    inner.notify.notified().await;
                }
            }
        }
        impl Remote {
            pub(super) fn cancel(&self) {
                let inner = &*self.0;
                inner.cancelled.store(true, Ordering::Release);
                inner.notify.notify_waiters();
            }
        }
    }
    mod io {
        use hyper::rt::{Read, ReadBufCursor, Write};
        use std::io;
        use std::pin::Pin;
        use std::task::{Context, Poll};
        use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
        use tokio::sync::OwnedSemaphorePermit;
        pub(crate) struct IoWithPermit<T> {
            io: T,
            _permit: OwnedSemaphorePermit,
        }
        impl<T> IoWithPermit<T> {
            #[inline]
            pub fn new(io: T, _permit: OwnedSemaphorePermit) -> Self {
                Self { io, _permit }
            }
        }
        impl<T> Drop for IoWithPermit<T> {
            fn drop(&mut self) {}
        }
        impl<T: AsyncRead + Unpin> Read for IoWithPermit<T> {
            fn poll_read(
                mut self: Pin<&mut Self>,
                context: &mut Context,
                mut buf: ReadBufCursor,
            ) -> Poll<io::Result<()>> {
                let len = unsafe {
                    let mut dest = ReadBuf::uninit(buf.as_mut());
                    let Poll::Ready(_) = Pin::new(&mut self.io)
                        .poll_read(context, &mut dest)? else {
                        return Poll::Pending;
                    };
                    dest.filled().len()
                };
                unsafe { buf.advance(len) };
                Poll::Ready(Ok(()))
            }
        }
        impl<T: AsyncWrite + Unpin> Write for IoWithPermit<T> {
            fn poll_write(
                mut self: Pin<&mut Self>,
                context: &mut Context<'_>,
                buf: &[u8],
            ) -> Poll<io::Result<usize>> {
                Pin::new(&mut self.io).poll_write(context, buf)
            }
            fn poll_flush(
                mut self: Pin<&mut Self>,
                context: &mut Context<'_>,
            ) -> Poll<io::Result<()>> {
                Pin::new(&mut self.io).poll_flush(context)
            }
            fn poll_shutdown(
                mut self: Pin<&mut Self>,
                context: &mut Context<'_>,
            ) -> Poll<io::Result<()>> {
                Pin::new(&mut self.io).poll_shutdown(context)
            }
            fn is_write_vectored(&self) -> bool {
                self.io.is_write_vectored()
            }
            fn poll_write_vectored(
                mut self: Pin<&mut Self>,
                context: &mut Context<'_>,
                bufs: &[io::IoSlice<'_>],
            ) -> Poll<io::Result<usize>> {
                Pin::new(&mut self.io).poll_write_vectored(context, bufs)
            }
        }
    }
    mod tls {
        use http::Version;
        use std::error::Error;
        use std::io;
        use tokio::io::{AsyncRead, AsyncWrite};
        use tokio::net::TcpStream;
        pub struct Alpn(Version);
        #[automatically_derived]
        impl ::core::cmp::Eq for Alpn {
            #[inline]
            #[doc(hidden)]
            #[coverage(off)]
            fn assert_receiver_is_total_eq(&self) -> () {
                let _: ::core::cmp::AssertParamIsEq<Version>;
            }
        }
        #[automatically_derived]
        impl ::core::marker::StructuralPartialEq for Alpn {}
        #[automatically_derived]
        impl ::core::cmp::PartialEq for Alpn {
            #[inline]
            fn eq(&self, other: &Alpn) -> bool {
                self.0 == other.0
            }
        }
        pub struct TcpAcceptor;
        pub trait Acceptor {
            type Io: AsyncRead + AsyncWrite;
            type Error: Error + Send;
            #[allow(dead_code)]
            fn accept(
                &self,
                io: TcpStream,
            ) -> impl Future<
                Output = Result<(Self::Io, Alpn), Self::Error>,
            > + Send + 'static;
        }
        impl Acceptor for TcpAcceptor {
            type Io = TcpStream;
            type Error = io::Error;
            #[allow(clippy::manual_async_fn)]
            fn accept(
                &self,
                _: TcpStream,
            ) -> impl Future<
                Output = Result<(Self::Io, Alpn), Self::Error>,
            > + Send + 'static {
                async {
                    ::core::panicking::panic("internal error: entered unreachable code")
                }
            }
        }
        #[allow(dead_code)]
        impl Alpn {
            pub const HTTP_2: Self = Self(Version::HTTP_2);
            pub const HTTP_11: Self = Self(Version::HTTP_11);
        }
    }
    use std::process::ExitCode;
    use std::time::Duration;
    use tokio::net::{TcpListener, ToSocketAddrs};
    use crate::app::{ServiceAdapter, Via};
    use crate::error::Error;
    use accept::accept;
    use io::IoWithPermit;
    use tls::TcpAcceptor;
    /// Serve an app over HTTP.
    ///
    pub struct Server<App> {
        app: Via<App>,
        config: ServerConfig,
    }
    pub(crate) struct ServerConfig {
        keep_alive: bool,
        max_buf_size: usize,
        max_connections: usize,
        max_request_size: usize,
        shutdown_timeout: Duration,
    }
    #[automatically_derived]
    impl ::core::fmt::Debug for ServerConfig {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            ::core::fmt::Formatter::debug_struct_field5_finish(
                f,
                "ServerConfig",
                "keep_alive",
                &self.keep_alive,
                "max_buf_size",
                &self.max_buf_size,
                "max_connections",
                &self.max_connections,
                "max_request_size",
                &self.max_request_size,
                "shutdown_timeout",
                &&self.shutdown_timeout,
            )
        }
    }
    impl<App> Server<App>
    where
        App: Send + Sync + 'static,
    {
        /// Creates a new server for the provided app.
        pub fn new(app: Via<App>) -> Self {
            Self {
                app,
                config: Default::default(),
            }
        }
        /// Enables or disables HTTP/1.1 persistent connections.
        ///
        /// When enabled, the server will allow clients to reuse a TCP connection
        /// for multiple requests. Disabling keep-alive forces the connection to be
        /// closed after each response is sent.
        ///
        /// Disabling keep-alive may reduce resource retention from idle clients but
        /// can increase connection overhead due to additional TCP and TLS handshakes.
        ///
        /// **Default:** `true`
        pub fn keep_alive(mut self, keep_alive: bool) -> Self {
            self.config.keep_alive = keep_alive;
            self
        }
        /// Sets the maximum size of the HTTP/1.1 connection read buffer.
        ///
        /// This buffer is used when reading and parsing the HTTP request line and
        /// headers from the client. It does **not** limit the size of the request
        /// body. Use [`Server::max_request_size`] to limit the maximum allowed
        /// request body size.
        ///
        /// **Default:** `16 KB`
        pub fn max_buf_size(mut self, max_buf_size: usize) -> Self {
            self.config.max_buf_size = max_buf_size;
            self
        }
        /// Sets the maximum number of concurrent connections that the server can
        /// accept.
        ///
        /// **Default:** `1000`
        pub fn max_connections(mut self, max_connections: usize) -> Self {
            self.config.max_connections = max_connections;
            self
        }
        /// Set the maximum request body size in bytes.
        ///
        /// **Default:** `100 MB`
        pub fn max_request_size(mut self, max_request_size: usize) -> Self {
            self.config.max_request_size = max_request_size;
            self
        }
        /// Set the amount of time in seconds that the server will wait for inflight
        /// connections to complete before shutting down.
        ///
        /// **Default:** `10s`
        pub fn shutdown_timeout(mut self, shutdown_timeout: Duration) -> Self {
            self.config.shutdown_timeout = shutdown_timeout;
            self
        }
        /// Listens for incoming connections at the provided address.
        ///
        /// Returns a future that resolves with a result containing an [`ExitCode`]
        /// when shutdown is requested.
        ///
        /// # Errors
        ///
        /// - If the server fails to bind to the provided address.
        /// - If the `rustls` feature is enabled and `rustls_config` is missing.
        ///
        /// # Exit Codes
        ///
        /// An [`ExitCode::SUCCESS`] can be viewed as a confirmation that every
        /// request was served before exiting the accept loop.
        ///
        /// An [`ExitCode::FAILURE`] is an indicator that an unrecoverable error
        /// occured which requires that the server be restarted in order to function
        /// as intended.
        ///
        /// If you are running your Via application as a daemon with a process
        /// supervisor such as upstart or systemd, you can use the exit code to
        /// determine whether or not the process should restart.
        ///
        /// If you are running your Via application in a cluster behind a load
        /// balancer you can use the exit code to properly configure node replacement
        /// and / or decommissioning logic.
        ///
        /// When high availability is mission-critical, and you are scaling your Via
        /// application both horizontally and vertically using a combination of the
        /// aforementioned deployment strategies, we recommend configuring a temporal
        /// threshold for the number of restarts caused by an [`ExitCode::FAILURE`].
        /// If the threshold is exceeded the cluster should immutably replace the
        /// node and the process supervisor should not make further attempts to
        /// restart the process.
        ///
        /// This approach significantly reduces the impact of environmental entropy
        /// on your application's availability while preventing conflicts between the
        /// process supervisor of an individual node and the replacement and
        /// decommissioning logic of the cluster.
        pub async fn listen(
            self,
            address: impl ToSocketAddrs,
        ) -> Result<ExitCode, Error> {
            let future = accept(
                TcpAcceptor,
                TcpListener::bind(address).await?,
                ServiceAdapter::new(self.config, self.app),
            );
            Ok(future.await)
        }
    }
    impl ServerConfig {
        pub fn keep_alive(&self) -> bool {
            self.keep_alive
        }
        pub fn max_buf_size(&self) -> usize {
            self.max_buf_size
        }
        pub fn max_connections(&self) -> usize {
            self.max_connections
        }
        pub fn max_request_size(&self) -> usize {
            self.max_request_size
        }
        pub fn shutdown_timeout(&self) -> Duration {
            self.shutdown_timeout
        }
    }
    impl Default for ServerConfig {
        fn default() -> Self {
            Self {
                keep_alive: true,
                max_buf_size: 16384,
                max_connections: 1000,
                max_request_size: 104_857_600,
                shutdown_timeout: Duration::from_secs(10),
            }
        }
    }
}
mod util {
    mod uri_encoding {
        use percent_encoding::percent_decode_str;
        use std::borrow::Cow;
        use crate::Error;
        pub enum UriEncoding {
            Percent,
            Unencoded,
        }
        #[automatically_derived]
        #[doc(hidden)]
        unsafe impl ::core::clone::TrivialClone for UriEncoding {}
        #[automatically_derived]
        impl ::core::clone::Clone for UriEncoding {
            #[inline]
            fn clone(&self) -> UriEncoding {
                *self
            }
        }
        #[automatically_derived]
        impl ::core::marker::Copy for UriEncoding {}
        #[automatically_derived]
        impl ::core::fmt::Debug for UriEncoding {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::write_str(
                    f,
                    match self {
                        UriEncoding::Percent => "Percent",
                        UriEncoding::Unencoded => "Unencoded",
                    },
                )
            }
        }
        impl UriEncoding {
            pub fn decode_as<'a>(
                &self,
                name: &str,
                input: &'a str,
            ) -> Result<Cow<'a, str>, Error> {
                if let Self::Unencoded = *self {
                    Ok(Cow::Borrowed(input))
                } else {
                    percent_decode_str(input)
                        .decode_utf8()
                        .map_err(|_| Error::invalid_utf8_sequence(name))
                }
            }
        }
    }
    pub use uri_encoding::UriEncoding;
}
pub use app::{Shared, Via, app};
pub use cookies::{Cookies, cookies};
pub use error::{Error, ResultExt, deny, rescue};
pub use guard::guard;
pub use middleware::{BoxFuture, Middleware, Result};
pub use next::{Continue, Next};
pub use request::{Payload, Request};
pub use response::{Finalize, Response};
pub use router::{connect, delete, get, head, options, patch, post, put, trace};
pub use server::Server;
pub use ws::ws;
