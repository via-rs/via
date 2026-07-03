//! HTTP request method predicates.
//!
//! Most predicates in this module are implemented for both [`Method`] and
//! [`Request`].
//!
//! The implementations for method are useful with projection combinators like
//! those found in the [`on`] module, while the implementations for request
//! make the common case straightforward.
//!
//! [`Method`] is the only contextual predicate in this module. When evaluated
//! against an input type, it returns a [contextual error] that can be into a
//! response with a `405` status code.
//!
//! ## Method Predicates and Method Switches
//!
//! The predicates in this module are not routing primitives.
//!
//! A method predicate classifies a request by method. When used with
//! [`barrier`], [`filter`], or [`flat_map`], it acts as stateless higher-order
//! middleware that decides whether the remaining subtree should be entered,
//! skipped, or denied.
//!
//! In contrast, the router's method [`Switch`] selects between terminal
//! middleware branches. Functions such as [`get`], [`post`], and [`put`] build
//! a decision tree for a responder attached to a route.
//!
//! Use a method predicate when middleware should be applied conditionally
//! before a responder is reached:
//!
//! ```
//! # use via::{Next, Request, guard};
//! # use via::guard::method;
//! # async fn cache(request: Request, next: Next) -> via::Result {
//! #     next.call(request).await
//! # }
//! # let mut app = via::app(());
//! app.middleware(guard::filter(method::is_safe(), cache));
//! ```
//!
//! Prefer a router switch when different HTTP methods should be handled by
//! different terminal middleware:
//!
//! ```
//! # mod users {
//! #   use via::{Next, Request};
//! #   pub async fn show(_: Request, _: Next) -> via::Result { todo!() }
//! #   pub async fn create(_: Request, _: Next) -> via::Result { todo!() }
//! # }
//! #
//! # let mut app = via::app(());
//! #
//! app.push("/users").assign(
//!     via::get(users::show)
//!         .post(users::create)
//!         .or_deny()
//! );
//! ```
//!
//! [`Method`]: http::Method
//! [`Request`]: crate::Request
//! [`Switch`]: crate::router::Switch
//! [`barrier`]: crate::guard::barrier
//! [contextual error]: crate::guard::error::MethodNotAllowed
//! [`filter`]: crate::guard::filter
//! [`flat_map`]: crate::guard::flat_map
//! [`get`]: crate::get
//! [`on`]: mod@crate::guard::on
//! [`post`]: crate::post
//! [`put`]: crate::put

use super::error::MethodNotAllowed;
use super::predicate::{Not, Predicate, not};
use crate::request::Request;

/// Match request methods that are
/// ["idempotent"](https://www.rfc-editor.org/rfc/rfc9110.html#name-idempotent-methods).
///
/// This predicate succeeds when [`Method::is_idempotent`] returns `true`.
///
/// Idempotent methods may be repeated without changing the intended effect of
/// the request. This is useful for middleware that should only run when retry
/// or replay semantics are acceptable.
///
/// [`Method::is_idempotent`]: http::Method::is_idempotent
pub struct Idempotent;

/// Match `GET`, `HEAD`, `OPTIONS`, and `TRACE` requests.
///
/// This predicate succeeds when [`Method::is_safe`] returns `true`.
/// ["Safe"](https://www.rfc-editor.org/rfc/rfc9110.html#name-safe-methods)
/// methods are read-only by convention.
///
/// [`Method::is_safe`]: http::Method::is_safe
pub struct Safe;

/// Match requests made with the provided method argument.
///
/// When evaluated against a [`Method`] this predicate behaves like a boolean.
/// When evaluated against a [`Request`], this predicate can return a
/// [contextual error] when the request method does not match.
///
/// [`Method`]: http::Method
/// [`Request`]: crate::Request
/// [contextual error]: crate::guard::error::MethodNotAllowed
pub struct Method {
    allow: http::Method,
}

macro_rules! methods {
    ($($vis:vis fn $name:ident($method:ident));+ $(;)?) => {
        $(
            #[doc = concat!("A predicate that succeeds for `", stringify!($method), "` requests.")]
            $vis fn $name() -> Method {
                Method { allow: http::Method::$method }
            }
        )+
    };
}

/// Succeeds for request methods that are
/// ["idempotent"](https://www.rfc-editor.org/rfc/rfc9110.html#name-idempotent-methods).
///
/// This predicate succeeds for methods where [`Method::is_idempotent`] returns
/// `true`.
///
/// # Example
///
/// ```
/// use http::Method;
/// use via::guard::{Predicate, method};
///
/// assert!(method::is_idempotent().cmp(&Method::GET).is_ok());
/// assert!(method::is_idempotent().cmp(&Method::PUT).is_ok());
///
/// assert!(method::is_idempotent().cmp(&Method::POST).is_err());
/// ```
///
/// [`Method::is_idempotent`]: http::Method::is_idempotent
pub fn is_idempotent() -> Idempotent {
    Idempotent
}

/// Succeeds for request methods that are not
/// ["safe"](https://www.rfc-editor.org/rfc/rfc9110.html#name-safe-methods).
///
/// # Example
///
/// ```
/// use http::Method;
/// use via::guard::{Predicate, method};
///
/// assert!(method::is_mutation().cmp(&Method::POST).is_ok());
/// assert!(method::is_mutation().cmp(&Method::PATCH).is_ok());
///
/// assert!(method::is_mutation().cmp(&Method::GET).is_err());
/// ```
///
/// [`is_safe`]: crate::guard::method::is_safe
pub fn is_mutation() -> Not<Safe> {
    not(is_safe())
}

/// Succeeds for `GET`, `HEAD`, `OPTIONS`, and `TRACE` requests.
///
/// ["Safe"](https://www.rfc-editor.org/rfc/rfc9110.html#name-safe-methods)
/// methods are read-only by convention.
///
/// # Example
///
/// ```
/// use http::Method;
/// use via::guard::{Predicate, method};
///
/// assert!(method::is_safe().cmp(&Method::GET).is_ok());
/// assert!(method::is_safe().cmp(&Method::HEAD).is_ok());
///
/// assert!(method::is_safe().cmp(&Method::POST).is_err());
/// ```
pub fn is_safe() -> Safe {
    Safe
}

methods! {
    pub fn connect(CONNECT);
    pub fn delete(DELETE);
    pub fn get(GET);
    pub fn head(HEAD);
    pub fn options(OPTIONS);
    pub fn patch(PATCH);
    pub fn post(POST);
    pub fn put(PUT);
    pub fn trace(TRACE);
}

impl Predicate<http::Method> for Method {
    type Error<'a> = MethodNotAllowed<'a>;

    fn cmp<'a>(&'a self, input: &http::Method) -> Result<(), Self::Error<'a>> {
        let method = &self.allow;

        if method == input {
            Ok(())
        } else {
            Err(MethodNotAllowed::new(method))
        }
    }
}

impl<App> Predicate<Request<App>> for Method {
    type Error<'a> = MethodNotAllowed<'a>;

    fn cmp<'a>(&'a self, request: &Request<App>) -> Result<(), Self::Error<'a>> {
        self.cmp(request.method())
    }
}

impl Predicate<http::Method> for Idempotent {
    type Error<'a> = ();

    fn cmp(&self, input: &http::Method) -> Result<(), ()> {
        if input.is_idempotent() {
            Ok(())
        } else {
            Err(())
        }
    }
}

impl<App> Predicate<Request<App>> for Idempotent {
    type Error<'a> = ();

    fn cmp(&self, request: &Request<App>) -> Result<(), ()> {
        self.cmp(request.method())
    }
}

impl Predicate<http::Method> for Safe {
    type Error<'a> = ();

    fn cmp(&self, input: &http::Method) -> Result<(), ()> {
        if input.is_safe() { Ok(()) } else { Err(()) }
    }
}

impl<App> Predicate<Request<App>> for Safe {
    type Error<'a> = ();

    fn cmp(&self, request: &Request<App>) -> Result<(), ()> {
        self.cmp(request.method())
    }
}
