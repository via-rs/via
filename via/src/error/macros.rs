/// Construct an error from a format string or error source.
///
/// This macro requires the [`http`] crate to be included as a dependency in
/// Cargo.toml.
///
/// # Examples
///
/// ```
/// use http::header::AUTHORIZATION;
/// use via::{Next, Request, err};
///
/// async fn authenticate(request: Request, next: Next) -> via::Result {
///     let authorization = request
///         .headers()
///         .get(AUTHORIZATION)
///         .ok_or_else(|| err!(401, "missing required header: authorization."))?;
///
///     // Authentication business logic...
///
///     next.call(request).await
/// }
/// ```
///
/// ### Customizing the error message.
///
/// The `err!` macro supports the same arguments as [`format!`] following the
/// status code.
///
/// ```
/// # use via::err;
/// err!(404, "user with id \"{}\" not found.", 12345);
/// ```
///
/// ### Decorate an existing error.
///
/// Existing errors can be passed as the second argument to the `err!` macro
/// instead of [`format!`] args.
///
/// ```
/// use std::io::{self, ErrorKind};
/// use via::err;
///
/// # fn example() -> via::Result<()> {
/// let result = Err(ErrorKind::InvalidInput.into());
/// result.map_err(|error: io::Error| err!(400, error))?;
/// # Ok(())
/// # }
/// ```
#[macro_export]
macro_rules! err {
    ($status:literal) => {
        $crate::err!(
            $crate::__error_expand_status_lit!($status),
            "{}.",
            $crate::__error_expand_status_lit!($status)
                .canonical_reason()
                .unwrap_or_default()
                .to_ascii_lowercase()
        )
    };
    ($status:literal $($args:tt)+) => {
        $crate::err!($crate::__error_expand_status_lit!($status) $($args)+)
    };
    ($status:expr, $message:literal $($args:tt)*) => {
        $crate::Error::new_with_status($status, format!($message $($args)*))
    };
    ($status:expr, $source:expr) => {
        $crate::Error::from_source_with_status($status, Box::new($source))
    };
}

/// Return an error from a format string or error source.
///
/// This macro requires the [`http`] crate to be included as a dependency in
/// Cargo.toml.
///
/// # Examples
///
/// ```
/// use http::header::AUTHORIZATION;
/// use via::{Next, Request, deny};
///
/// async fn authenticate(request: Request, next: Next) -> via::Result {
///     let Some(authorization) = request.headers().get(AUTHORIZATION) else {
///         deny!(401, "missing required header: authorization.");
///     };
///
///     // Authentication business logic...
///
///     next.call(request).await
/// }
/// ```
///
#[macro_export]
macro_rules! deny {
    ($status:literal) => {
        $crate::deny!(
            $crate::__error_expand_status_lit!($status),
            "{}.",
            $crate::__error_expand_status_lit!($status)
                .canonical_reason()
                .unwrap_or_default()
                .to_ascii_lowercase()
        )
    };
    ($status:literal $($args:tt)+) => {
        $crate::deny!($crate::__error_expand_status_lit!($status) $($args)+)
    };
    ($status:expr, $message:literal $($args:tt)*) => {
        return Err($crate::Error::new_with_status($status, format!($message $($args)*)))
    };
    ($status:expr, $source:expr) => {
        return Err($crate::Error::from_source_with_status($status, Box::new($source)))
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __error_expand_status_lit {
    (400) => {
        http::StatusCode::BAD_REQUEST
    };
    (401) => {
        http::StatusCode::UNAUTHORIZED
    };
    (402) => {
        http::StatusCode::PAYMENT_REQUIRED
    };
    (403) => {
        http::StatusCode::FORBIDDEN
    };
    (404) => {
        http::StatusCode::NOT_FOUND
    };
    (405) => {
        http::StatusCode::METHOD_NOT_ALLOWED
    };
    (406) => {
        http::StatusCode::NOT_ACCEPTABLE
    };
    (407) => {
        http::StatusCode::PROXY_AUTHENTICATION_REQUIRED
    };
    (408) => {
        http::StatusCode::REQUEST_TIMEOUT
    };
    (409) => {
        http::StatusCode::CONFLICT
    };
    (410) => {
        http::StatusCode::GONE
    };
    (411) => {
        http::StatusCode::LENGTH_REQUIRED
    };
    (412) => {
        http::StatusCode::PRECONDITION_FAILED
    };
    (413) => {
        http::StatusCode::PAYLOAD_TOO_LARGE
    };
    (414) => {
        http::StatusCode::URI_TOO_LONG
    };
    (415) => {
        http::StatusCode::UNSUPPORTED_MEDIA_TYPE
    };
    (416) => {
        http::StatusCode::RANGE_NOT_SATISFIABLE
    };
    (417) => {
        http::StatusCode::EXPECTATION_FAILED
    };
    (418) => {
        http::StatusCode::IM_A_TEAPOT
    };
    (421) => {
        http::StatusCode::MISDIRECTED_REQUEST
    };
    (422) => {
        http::StatusCode::UNPROCESSABLE_ENTITY
    };
    (423) => {
        http::StatusCode::LOCKED
    };
    (424) => {
        http::StatusCode::FAILED_DEPENDENCY
    };
    (426) => {
        http::StatusCode::UPGRADE_REQUIRED
    };
    (428) => {
        http::StatusCode::PRECONDITION_REQUIRED
    };
    (429) => {
        http::StatusCode::TOO_MANY_REQUESTS
    };
    (431) => {
        http::StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE
    };
    (451) => {
        http::StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS
    };
    (500) => {
        http::StatusCode::INTERNAL_SERVER_ERROR
    };
    (501) => {
        http::StatusCode::NOT_IMPLEMENTED
    };
    (502) => {
        http::StatusCode::BAD_GATEWAY
    };
    (503) => {
        http::StatusCode::SERVICE_UNAVAILABLE
    };
    (504) => {
        http::StatusCode::GATEWAY_TIMEOUT
    };
    (505) => {
        http::StatusCode::HTTP_VERSION_NOT_SUPPORTED
    };
    (506) => {
        http::StatusCode::VARIANT_ALSO_NEGOTIATES
    };
    (507) => {
        http::StatusCode::INSUFFICIENT_STORAGE
    };
    (508) => {
        http::StatusCode::LOOP_DETECTED
    };
    (510) => {
        http::StatusCode::NOT_EXTENDED
    };
    (511) => {
        http::StatusCode::NETWORK_AUTHENTICATION_REQUIRED
    };
    ($code:literal $($args:tt)*) => {{
        const CODE: u16 = $code;
        const _: () = assert!(
            CODE >= 400 && CODE <= 599,
            "status code must be in 400..=599 for errors.",
        );

        let Ok(status) = http::StatusCode::from_u16(CODE) else {
            unreachable!()
        };

        status
    }};
}
