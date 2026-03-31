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
#[derive(Debug)]
struct SetCookieError;

/// Parse request cookies and serialize response cookies.
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
/// use via::{Cookies, Error, Next, Request, Response, Server};
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
///         None => (true, request.param("name").percent_decode().ok_or_bad_request()?),
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
///     app.middleware(Cookies::new().allow("name").decode());
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
/// use via::{Cookies, Error, Next, Payload, Request, Response, Server};
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
///     app.middleware(Cookies::new().allow("via-session"));
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

/// Returns middleware that provides support for unencoded request and
/// response cookies.
///
/// # Example
///
/// ```
/// let mut app = via::app(());
/// app.middleware(via::cookies());
/// ```
///
pub fn cookies() -> Cookies {
    Cookies::new()
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
    /// Add the provided cookie name to the allow list.
    ///
    /// By default, the Cookies middleware ignores cookies with names that are
    /// not explicitly allowed. This filters out irrelevant cookies and keeps
    /// the number of cookies in the request and response cookie jars bounded.
    ///
    /// # Example
    ///
    /// ```
    /// # use via::{Cookies};
    /// # let mut app = via::app(());
    /// app.middleware(Cookies::new().allow("via-session"));
    /// ```
    ///
    pub fn allow(mut self, name: impl AsRef<str>) -> Self {
        self.allow.insert(name.as_ref().to_owned());
        self
    }

    /// Specify that cookies should be percent-decoded when parsed and percent-
    /// encoded when serialized as a Set-Cookie header.
    ///
    /// # Example
    ///
    /// ```
    /// # use via::{Cookies};
    /// # let mut app = via::app(());
    /// app.middleware(Cookies::new().allow("via-session").decode());
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

    fn parse(&self, input: &str) -> impl Iterator<Item = Cookie<'static>> {
        split_parse(&self.encoding, input).filter_map(|result| {
            let shared = match result {
                Ok(cookie) => cookie,
                Err(error) => {
                    if cfg!(debug_assertions) {
                        eprintln!("warn(cookies): {}", error);
                    }

                    return None;
                }
            };

            self.allow
                .contains(shared.name())
                .then(|| shared.into_owned())
        })
    }
}

impl<App> Middleware<App> for Cookies {
    fn call(&self, mut request: Request<App>, next: Next<App>) -> BoxFuture {
        let existing = request.headers().get(COOKIE).and_then(|header| {
            let input = header.to_str().ok()?;
            Some(self.parse(input).collect::<Vec<_>>())
        });

        if let Some(cookies) = existing.as_deref() {
            cookies.iter().cloned().for_each(|cookie| {
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

            cookies.delta().try_for_each(|cookie| {
                let value = HeaderValue::from_maybe_shared::<Bytes>(match encoding {
                    UriEncoding::Percent => cookie.encoded().to_string().into(),
                    UriEncoding::Unencoded => cookie.to_string().into(),
                })?;

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
        write!(
            f,
            "An error occurred while writing a Set-Cookie header to a response."
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
