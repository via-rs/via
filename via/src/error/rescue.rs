//! Recover from service errors with capability-bounded policies.
//!
//! Unlike traditional exception handlers or recovery callbacks, rescue
//! policies cannot arbitrarily construct responses. Instead, they observe a
//! recovered [`Error`] and request predefined rendering behavior such as JSON
//! output, canonical reason phrases, or connection termination.
//!
//! This separation of concerns keeps response construction within Via while
//! still allowing applications to customize how errors are presented and
//! reported. Policies may also perform side effects such as logging or
//! telemetry without influencing the rendered response.
//!
//! Policies compose declaratively:
//!
//! ```no_run
//! use via::error::rescue;
//!
//! let policy = rescue::canonical_reason_phrase()
//!     .and(rescue::connection_close())
//!     .and(rescue::json())
//!     .build();
//! ```
//!
//! The resulting middleware may be applied to any portion of the middleware
//! stack, allowing different trust boundaries to expose different recovery
//! policies.

use bitflags::bitflags;
use http::StatusCode;
use http::header::{ALLOW, CONNECTION};
use std::borrow::Cow;
use std::fmt::Display;
use std::sync::Arc;

use super::{Error, ErrorSourceRef, Errors};
use crate::middleware::{BoxFuture, Middleware};
use crate::response::{Finalize, Response, ResponseBuilder};
use crate::util::sealed;
use crate::{Next, Request};

/// A bounded error recovery policy.
///
/// A `Policy` determines how a recovered [`Error`] should be rendered without
/// granting authority to construct arbitrary responses.
///
/// Policies receive the recovered error together with the accumulated
/// [`PolicyFlags`], and return the updated flags. Multiple policies may be
/// composed with [`PolicyBuilder::and`].
///
/// This trait is sealed and cannot be implemented outside of Via.
pub trait Policy: sealed::Sealed {
    fn apply(&self, error: &Error, flags: PolicyFlags) -> PolicyFlags;
}

/// Compose two recovery policies.
///
/// Policies are applied from left to right.
pub struct And<T, U>(T, U);

/// Use the HTTP status reason phrase as the error message.
///
/// When enabled, the rendered response body contains the canonical reason
/// phrase associated with the response status code instead of the original
/// error message.
pub struct CanonicalReasonPhrase;

/// Instruct the client to close the connection.
///
/// When enabled, the rendered response includes a `Connection: close` header.
pub struct ConnectionClose;

/// Render recovered errors as JSON.
///
/// When enabled, the error is serialized to JSON that is compatible with both
/// GraphQL and JSON::API.
pub struct Json;

/// Incrementally construct a recovery policy.
///
/// Builders may be combined with [`PolicyBuilder::and`] before being converted
/// into a [`Rescue`] middleware with [`PolicyBuilder::build`].
pub struct PolicyBuilder<T> {
    policy: T,
}

/// The accumulated state of a recovery policy.
///
/// `PolicyFlags` records which rendering behaviors have been requested by the
/// composed recovery policy.
pub struct PolicyFlags {
    mask: PolicyMask,
}

/// Inspect the recovered error before a response is generated from it.
///
/// The callback receives an opaque display view of the recovered error.
///
/// This allows diagnostics such as logging or telemetry without exposing the
/// structured error metadata used by Via to render the response.
pub struct Inspect<F> {
    op: F,
}

/// Recover from errors that occur in subsequent middleware.
pub struct Rescue<T> {
    policy: Arc<T>,
}

struct Render<'a> {
    flags: &'a PolicyMask,
    error: &'a Error,
}

bitflags! {
    #[derive(Clone, Copy, Eq, PartialEq)]
    struct PolicyMask: u8 {
        const CANONICAL_REASON_PHRASE = 1 << 3;
        const CONNECTION_CLOSE = 1 << 4;
        const JSON_RESPONSE = 1 << 5;
    }
}

sealed!(
    And<T, U>,
    CanonicalReasonPhrase,
    ConnectionClose,
    Inspect<F>,
    Json
);

/// Render the canonical HTTP reason phrase instead of the original error.
pub fn canonical_reason_phrase() -> PolicyBuilder<CanonicalReasonPhrase> {
    PolicyBuilder {
        policy: CanonicalReasonPhrase,
    }
}

/// Add a `Connection: close` header to generated responses.
pub fn connection_close() -> PolicyBuilder<ConnectionClose> {
    PolicyBuilder {
        policy: ConnectionClose,
    }
}

/// Inspect the recovered error before a response is generated from it.
///
/// The callback receives an opaque display view of the recovered error.
///
/// This allows diagnostics such as logging or telemetry without exposing the
/// structured error metadata used by Via to render the response.
pub fn inspect<F>(op: F) -> PolicyBuilder<Inspect<F>>
where
    F: Fn(&dyn Display) + Copy + Send + Sync + 'static,
{
    PolicyBuilder {
        policy: Inspect { op },
    }
}

/// Render recovered errors as JSON.
pub fn json() -> PolicyBuilder<Json> {
    PolicyBuilder { policy: Json }
}

impl<T> PolicyBuilder<T> {
    /// Combine two recovery policies.
    ///
    /// Policies are applied in the order they are combined.
    pub fn and<U>(self, other: PolicyBuilder<U>) -> PolicyBuilder<And<T, U>> {
        PolicyBuilder {
            policy: And(self.policy, other.policy),
        }
    }

    /// Build a [`Rescue`] middleware from this policy.
    pub fn build(self) -> Rescue<T> {
        Rescue {
            policy: Arc::new(self.policy),
        }
    }
}

impl<T: Policy, U: Policy> Policy for And<T, U> {
    fn apply(&self, error: &Error, flags: PolicyFlags) -> PolicyFlags {
        let flags = self.0.apply(error, flags);
        self.1.apply(error, flags)
    }
}

impl Policy for CanonicalReasonPhrase {
    fn apply(&self, _: &Error, flags: PolicyFlags) -> PolicyFlags {
        flags.set(PolicyMask::CANONICAL_REASON_PHRASE)
    }
}

impl Policy for ConnectionClose {
    fn apply(&self, _: &Error, flags: PolicyFlags) -> PolicyFlags {
        flags.set(PolicyMask::CONNECTION_CLOSE)
    }
}

impl<F> Policy for Inspect<F>
where
    F: Fn(&dyn Display) + Send + Sync + 'static,
{
    fn apply(&self, error: &Error, flags: PolicyFlags) -> PolicyFlags {
        (self.op)(error);
        flags
    }
}

impl Policy for Json {
    fn apply(&self, _: &Error, flags: PolicyFlags) -> PolicyFlags {
        flags.set(PolicyMask::JSON_RESPONSE)
    }
}

impl PolicyFlags {
    fn new() -> Self {
        Self {
            mask: PolicyMask::empty(),
        }
    }

    fn set(self, flag: PolicyMask) -> Self {
        Self {
            mask: self.mask | flag,
        }
    }
}

impl<T, App> Middleware<App> for Rescue<T>
where
    T: Policy + Send + Sync + 'static,
{
    fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture {
        let policy = Arc::clone(&self.policy);
        let future = next.call(request);

        Box::pin(async move {
            future.await.or_else(|error| {
                let flags = policy.apply(&error, PolicyFlags::new());

                match Render::new(&flags.mask, &error).finalize(Response::build()) {
                    Ok(response) => Ok(response),

                    // In release builds, residual errors are not logged.
                    // Therefore, we branch at build-time on the match arm.
                    //
                    #[cfg(not(debug_assertions))]
                    Err(_) => Ok(error.into()),

                    #[cfg(debug_assertions)]
                    Err(residual) => {
                        log!(warn(rescue = 0), "a residual error occurred in rescue");
                        log!(warn(rescue = 1), "{}", &residual);
                        Ok(error.into())
                    }
                }
            })
        })
    }
}

impl<'a> Render<'a> {
    fn new(flags: &'a PolicyMask, error: &'a Error) -> Self {
        Self { flags, error }
    }
}

impl Finalize for Render<'_> {
    fn finalize(self, mut builder: ResponseBuilder) -> Result<Response, Error> {
        builder = builder.status(self.error.status);

        if let ErrorSourceRef::AllowMethod(error) = self.error.as_source()
            && self.error.status == StatusCode::METHOD_NOT_ALLOWED
            && let Some(allow) = error.allows()
        {
            builder = builder.header(ALLOW, allow);
        }

        if self.flags.contains(PolicyMask::CONNECTION_CLOSE) {
            builder = builder.header(CONNECTION, "close");
        }

        if self.flags.contains(PolicyMask::JSON_RESPONSE) {
            if self.flags.contains(PolicyMask::CANONICAL_REASON_PHRASE)
                && let Some(reason_phrase) = self.error.status.canonical_reason()
            {
                let mut errors = Errors::new(self.error.status);
                errors.push(Cow::Borrowed(reason_phrase));
                builder.json(&errors)
            } else {
                let errors = self.error.repr_json();
                builder.json(&errors)
            }
        } else if self.flags.contains(PolicyMask::CANONICAL_REASON_PHRASE)
            && let Some(reason_phrase) = self.error.status.canonical_reason()
        {
            builder.text(reason_phrase)
        } else {
            builder.text(self.error.to_string())
        }
    }
}
