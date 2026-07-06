//! Interact with incoming requests to your service.

pub mod params;

mod payload;
mod query;

pub use params::{PathParams, QueryParams};
pub use payload::{Aggregate, Coalesce, Payload, Payloadz, RequestBody};

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

/// The component parts of an HTTP request head and their derivatives.
///
/// `Envelope` allows the request head and derived metadata to outlive the
/// request container. This is particularly useful for long-lived connections
/// (such as WebSocket upgrades) and security-sensitive endpoints where the
/// request body should be consumed as quickly as possible.
///
/// Retaining the request head and its derived metadata allows them to be used
/// tangentially to the fulfillment of the request, for example when populating
/// an audit log as an asynchronous post-processing step.
///
/// # Example
///
/// ```no_run
/// use serde::{Deserialize, Serialize};
/// use std::process::ExitCode;
/// use via::request::{Envelope, Payloadz};
/// use via::{Error, Response, ResultExt, Server};
/// use zeroize::Zeroizing;
///
/// type Request = via::Request<Unicorn>;
/// type Next = via::Next<Unicorn>;
///
/// /// A generic JSON payload.
/// #[derive(Deserialize, Serialize)]
/// struct Json<T> {
///     data: T,
/// }
///
/// /// Our billion dollar application.
/// struct Unicorn {
///     database: Database,
///     telemetry: Telemetry,
/// }
///
/// /// The arguments deserialized from a request to POST /auth.
/// #[derive(Deserialize)]
/// struct LoginParams {
///     username: String,
///     password: Zeroizing<String>,
/// }
///
/// /// Authenticate with a username and password.
/// async fn login(request: Request, _: Next) -> via::Result {
///     // Get a tuple containing the fields of `request`.
///     let (envelope, body, app) = request.into_parts();
///
///     // Aggregate the frames of the request body into a
///     // contiguous block of memory. Timeout after 5s.
///     let body = body.coalesce().timeout_after_secs(5).await?;
///
///     // Find an existing user matching the params in `body`.
///     let me = {
///         // Deserialize `LoginParams` from JSON. Then, zeroize
///         // the bytes in `body`.
///         let params = body.be_z_json::<Json<LoginParams>>()?;
///         //                                 ^^^^^^^^^^^
///         // Password zeroization is enforced by the type.
///         app.database.authenticate(params.data).await?
///     };
///
///     // Spawn a detached task that takes ownership of the request
///     // envelope, a cloned copy of the active user, and a shared
///     // handle to your application.
///     tokio::spawn({
///         // Prepare the telemetry task dependencies.
///         let action = Action::Login {
///             request: envelope,
///             user: me.clone(),
///         };
///
///         // Then, report to your favorite telemetry service.
///         async move {
///             app.telemetry.notify(action).await;
///         }
///     });
///
///     // Only `me` needs drop at this point. The password is
///     // zeroed and is no longer a risk.
///     //
///     // Respond with the active user serialized to JSON.
///     Response::build().json(&Json { data: me })
/// }
///
/// #[tokio::main]
/// async fn main() -> Result<ExitCode, Error> {
///     // Define our application.
///     let mut app = via::app(Unicorn {
///         database: Database,
///         telemetry: Telemetry,
///     });
///
///     // Accept POST requests to /auth with the login fn.
///     app.route("/auth", via::post(login).or_deny());
///
///     // Listen for incoming connections at http://localhost:8080/.
///     // We recommend enabling a TLS-backend in production.
///     Server::new(app).listen(("127.0.0.1", 8080)).await
/// }
/// #
/// # /// A placeholder type for your favorite database pool.
/// # struct Database;
/// #
/// # /// A placeholder type for your favorite telemetry service.
/// # struct Telemetry;
/// #
/// # /// Represents an auditable action.
/// # enum Action {
/// #     Login { request: Envelope, user: ActiveUser },
/// # }
/// #
/// #
/// # #[derive(Clone, Serialize)]
/// # struct ActiveUser {
/// #     id: u64,
/// #     username: String,
/// # }
/// #
/// #
/// # impl Database {
/// #     // Friendly reminder: never store a password as plain text!
/// #     async fn authenticate(&self, _: LoginParams) -> via::Result<ActiveUser> {
/// #         todo!("implement username + password authentication.")
/// #     }
/// # }
/// #
/// # impl Telemetry {
/// #     async fn notify(&self, action: Action) {
/// #         todo!("integrate with your favorite telemetry service.")
/// #     }
/// # }
/// ```
pub struct Envelope {
    parts: Parts,
    params: Vec<via_router::PathParam>,
    cookies: CookieJar,
}

/// Represents an HTTP request to your application.
///
/// A `Request` consists of the request head and derived metadata stored in an
/// [`Envelope`], the [`RequestBody`], and a [`Shared`] handle to your
/// application.
///
/// The shared handle to your app provides a form of dependency injection,
/// permitting per-request access to singleton resources such as a database
/// pool. The argument passed to [`via::app`](crate::app::app) defines the
/// resources that are made available to each request as the generic `App` type
/// parameter. Connections tasks are processed asynchronously across worker
/// threads with work-stealing scheduler. Therefore, `App` must be both
/// [`Send`] and [`Sync`].
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

    /// Parse the query string in the request [`Uri`] into key-value pairs
    /// where values are spans of the query string. Then, attempt to convert
    /// the [`QueryParams`] into type `T`.
    pub fn query<'a, T>(&'a self) -> crate::Result<T>
    where
        T: TryFrom<QueryParams<'a>, Error = Error>,
    {
        T::try_from(QueryParams::new(self.uri().query()))
    }

    /// Attempt to convert references to the path params in the request [`Uri`]
    /// into type `T`.
    ///
    /// This method borrows a `str` to a the path in the request uri once
    /// as opposed to once per param, it is the preferred method of
    /// deserializing params for requests with more than one dynamic parameter.
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
        #[derive(Debug)]
        struct CookieJar;

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
    pub(crate) fn new(envelope: Envelope, body: RequestBody, app: Shared<App>) -> Self {
        Self {
            envelope,
            body,
            app,
        }
    }

    /// Returns a reference to the argument passed to [`via::app`](crate::app).
    #[inline]
    pub fn app(&self) -> &App {
        &self.app
    }

    /// Returns an owned, reference-counting pointer to the argument passed to
    /// [`via::app`](crate::app).
    pub fn app_owned(&self) -> Shared<App> {
        self.app.clone()
    }

    delegate! {
        to self.envelope {
            /// Returns a reference to the request's method.
            pub fn method(&self) -> &Method;

            /// Returns a reference to the request's URI.
            pub fn uri(&self) -> &Uri;

            /// Returns the HTTP version that was used to make the request.
            pub fn version(&self) -> Version;

            /// Returns a reference to the request's headers.
            pub fn headers(&self) -> &HeaderMap;

            /// Returns reference to the cookies associated with the request.
            pub fn cookies(&self) -> &CookieJar;

            /// Returns a mutable reference to the cookies associated with the request.
            pub fn cookies_mut(&mut self) -> &mut CookieJar;

            /// Returns a reference to the associated extensions.
            pub fn extensions(&self) -> &Extensions;

            /// Returns a mutable reference to the associated extensions.
            pub fn extensions_mut(&mut self) -> &mut Extensions;

            /// Returns reference to the cookies associated with the request.
            pub fn param<'b>(&self, name: &'b str) -> PathParam<'_, 'b>;

            /// Parse the query string in the request [`Uri`] into key-value pairs
            /// where values are spans of the query string. Then, attempt to convert
            /// the [`QueryParams`] into type `T`.
            pub fn query<'a, T>(&'a self) -> crate::Result<T>
            where
                T: TryFrom<QueryParams<'a>, Error = Error>;

            /// Attempt to convert references to the path params in the request
            /// [`Uri`] into type `T`.
            ///
            /// This method borrows a `str` to a the path in the request uri
            /// once as opposed to once per param, it is the preferred method
            /// of deserializing params for requests with more than one dynamic
            /// parameter.
            pub fn params<'a, T>(&'a self) -> crate::Result<T>
            where
                T: TryFrom<PathParams<'a>>,
                Error: From<T::Error>;
        }
    }

    /// Consumes the request and returns a tuple containing a future that
    /// resolves with the data and trailers of the body as well as a shared
    /// copy of `App`.
    ///
    pub fn into_future(self) -> (Coalesce, Shared<App>) {
        (Coalesce::new(self.body), self.app)
    }

    /// Consumes the request and returns a tuple containing its parts.
    #[inline]
    pub fn into_parts(self) -> (Envelope, RequestBody, Shared<App>) {
        (self.envelope, self.body, self.app)
    }
}

impl<App> Debug for Request<App> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Request")
            .field("envelope", &self.envelope)
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
