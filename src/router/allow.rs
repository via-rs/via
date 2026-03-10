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

#[derive(Debug)]
pub(crate) struct MethodNotAllowed {
    allow: Mask,
    method: Mask,
}

trait Predicate {
    fn matches(&self, other: &Mask) -> bool;
}

bitflags! {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct Mask: u16 {
        const CONNECT = 1 << 0;
        const DELETE  = 1 << 1;
        const GET     = 1 << 2;
        const HEAD    = 1 << 3;
        const OPTIONS = 1 << 4;
        const PATCH   = 1 << 5;
        const POST    = 1 << 6;
        const PUT     = 1 << 7;
        const TRACE   = 1 << 8;
    }
}

macro_rules! methods {
    ($($vis:vis fn $name:ident($method:ident));+ $(;)?) => {
        $(
            #[doc = concat!(
                "Route `",
                stringify!($method),
                "` requests to the provided middleware."
            )]
            $vis fn $name<T>(middleware: T) -> Branch<Allow<T>, Continue> {
                let mask = Mask::$method;

                Branch {
                    middleware: Allow { middleware, mask },
                    or_else: Continue,
                    mask,
                }
            }
        )+
    };
    ($($vis:vis fn $name:ident($self:ident, $method:ident));+ $(;)?) => {
        $($vis fn $name<F>($self, middleware: F) -> Branch<Allow<F>, Self> {
            let mask = Mask::$method;

            Branch {
                mask: $self.mask | mask,
                or_else: $self,
                middleware: Allow { middleware, mask },
            }
        })+
    };
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

impl<T, U> Branch<T, U> {
    methods! {
        pub fn connect(self, CONNECT);
        pub fn delete(self, DELETE);
        pub fn get(self, GET);
        pub fn head(self, HEAD);
        pub fn options(self, OPTIONS);
        pub fn patch(self, PATCH);
        pub fn post(self, POST);
        pub fn put(self, PUT);
        pub fn trace(self, TRACE);
    }

    /// Returns a `405 Method Not Allowed` response if the request method is
    /// not supported.
    ///
    /// # Example
    ///
    /// ```
    /// # use via::{Next, Request, Response};
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
    pub fn allows(&self) -> String {
        self.allow.iter().fold(String::new(), |allow, mask| {
            let Some(method) = mask.as_str() else {
                return allow;
            };

            if allow.is_empty() {
                allow + method
            } else {
                allow + ", " + method
            }
        })
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
            write!(f, "method not allowed: \"{}\"", method)
        } else {
            write!(f, "method not allowed")
        }
    }
}
