/// Return with a new error or decorate an existing one.
///
/// # Examples
///
/// Return an error that uses the canonical reason prase of the provided
/// status code.
///
/// ```
/// use http::header::AUTHORIZATION;
/// use via::{Next, Request};
///
/// async fn authenticate(request: Request, next: Next) -> via::Result {
///     let Some(jwt) = request.envelope().headers().get(AUTHORIZATION) else {
///         via::raise!(401, message = "Missing required header: Authorization.");
///     };
///
///     // Insert JWT-based authentication strategy here.
///
///     next.call(request).await
/// }
/// ```
///
/// ### Decorate an existing error.
///
/// The generic impl of
/// [`From<E> for Error`](super::Error#impl-From<E>-for-Error)
/// uses `500` as the status code. It is often times desirable to provide a
/// more appropriate status code when the error is in context rather than using
/// dynamic typing to determine the status code that should be used in a
/// [`Rescue`](super::Rescue)
/// callback.
///
/// ```
/// use std::io;
/// use via::raise;
///
/// fn invalid_input() -> io::Result<()> {
///     Err(io::ErrorKind::InvalidInput.into())
/// }
///
/// // Unboxed error types are passed as the second positional argument.
/// # fn implicit_box() -> via::Result<()> {
/// invalid_input().or_else(|error| raise!(400, error))?;
/// # Ok(())
/// # }
///
/// # fn explicit_box() -> via::Result<()> {
/// // If the error source is already boxed, specify so to avoid allocating.
/// invalid_input().or_else(|error| raise!(400, boxed = Box::new(error)))?;
/// # Ok(())
/// # }
/// ```
///
/// ### Customizing the error message.
///
/// The `raise!` macro also allows you to provide a custom error message. The
/// message argument accepts `impl Into<String>`. Passing an owned `String` is
/// no less efficient than passing a `message = &'static str`.
///
/// ```
/// # use via::raise;
/// // Implicit allocation for message:
/// # fn implicit_alloc() -> via::Result {
/// raise!(404, message = "Could not find a user with the provided id.");
/// # }
///
/// // Explicit allocation for message:
/// # fn explicit_alloc() -> via::Result {
/// raise!(404, message = format!("User with id: {} does not exist.", 12345));
/// # }
/// ```
///
#[macro_export]
macro_rules! raise {
    (#[ctor] $status:expr, message = $message:expr $(,)?) => {
        return Err($crate::Error::with_status($status, $message))
    };
    (#[ctor] $status:expr, boxed = $source:expr $(,)?) => {
        return Err($crate::Error::from_source($status, $source))
    };
    (#[ctor] $status:expr, $source:expr $(,)?) => {
        return Err($crate::Error::from_source($status, Box::new($source)))
    };
    (#[ctor] $status:expr) => {{
        let status = $status;
        let message = status.canonical_reason().unwrap_or_default().to_owned();
        return Err($crate::Error::with_status(status, message))
    }};

    (|| $($args:tt)*) => { (|| { $crate::raise!($($args)*) })() };

    (boxed = $source:expr $(,)?) => { $crate::raise!(500, boxed = $source) };
    (message = $message:expr $(,)?) => { $crate::raise!(500, message = $message) };

    (400 $($args:tt)*) => { $crate::raise!(BAD_REQUEST $($args)*) };
    (401 $($args:tt)*) => { $crate::raise!(UNAUTHORIZED $($args)*) };
    (402 $($args:tt)*) => { $crate::raise!(PAYMENT_REQUIRED $($args)*) };
    (403 $($args:tt)*) => { $crate::raise!(FORBIDDEN $($args)*) };
    (404 $($args:tt)*) => { $crate::raise!(NOT_FOUND $($args)*) };
    (405 $($args:tt)*) => { $crate::raise!(METHOD_NOT_ALLOWED $($args)*) };
    (406 $($args:tt)*) => { $crate::raise!(NOT_ACCEPTABLE $($args)*) };
    (407 $($args:tt)*) => { $crate::raise!(PROXY_AUTHENTICATION_REQUIRED $($args)*) };
    (408 $($args:tt)*) => { $crate::raise!(REQUEST_TIMEOUT $($args)*) };
    (409 $($args:tt)*) => { $crate::raise!(CONFLICT $($args)*) };
    (410 $($args:tt)*) => { $crate::raise!(GONE $($args)*) };
    (411 $($args:tt)*) => { $crate::raise!(LENGTH_REQUIRED $($args)*) };
    (412 $($args:tt)*) => { $crate::raise!(PRECONDITION_FAILED $($args)*) };
    (413 $($args:tt)*) => { $crate::raise!(PAYLOAD_TOO_LARGE $($args)*) };
    (414 $($args:tt)*) => { $crate::raise!(URI_TOO_LONG $($args)*) };
    (415 $($args:tt)*) => { $crate::raise!(UNSUPPORTED_MEDIA_TYPE $($args)*) };
    (416 $($args:tt)*) => { $crate::raise!(RANGE_NOT_SATISFIABLE $($args)*) };
    (417 $($args:tt)*) => { $crate::raise!(EXPECTATION_FAILED $($args)*) };
    (418 $($args:tt)*) => { $crate::raise!(IM_A_TEAPOT $($args)*) };
    (421 $($args:tt)*) => { $crate::raise!(MISDIRECTED_REQUEST $($args)*) };
    (422 $($args:tt)*) => { $crate::raise!(UNPROCESSABLE_ENTITY $($args)*) };
    (423 $($args:tt)*) => { $crate::raise!(LOCKED $($args)*) };
    (424 $($args:tt)*) => { $crate::raise!(FAILED_DEPENDENCY $($args)*) };
    (426 $($args:tt)*) => { $crate::raise!(UPGRADE_REQUIRED $($args)*) };
    (428 $($args:tt)*) => { $crate::raise!(PRECONDITION_REQUIRED $($args)*) };
    (429 $($args:tt)*) => { $crate::raise!(TOO_MANY_REQUESTS $($args)*) };
    (431 $($args:tt)*) => { $crate::raise!(REQUEST_HEADER_FIELDS_TOO_LARGE $($args)*) };
    (451 $($args:tt)*) => { $crate::raise!(UNAVAILABLE_FOR_LEGAL_REASONS $($args)*) };
    (500 $($args:tt)*) => { $crate::raise!(INTERNAL_SERVER_ERROR $($args)*) };
    (501 $($args:tt)*) => { $crate::raise!(NOT_IMPLEMENTED $($args)*) };
    (502 $($args:tt)*) => { $crate::raise!(BAD_GATEWAY $($args)*) };
    (503 $($args:tt)*) => { $crate::raise!(SERVICE_UNAVAILABLE $($args)*) };
    (504 $($args:tt)*) => { $crate::raise!(GATEWAY_TIMEOUT $($args)*) };
    (505 $($args:tt)*) => { $crate::raise!(HTTP_VERSION_NOT_SUPPORTED $($args)*) };
    (506 $($args:tt)*) => { $crate::raise!(VARIANT_ALSO_NEGOTIATES $($args)*) };
    (507 $($args:tt)*) => { $crate::raise!(INSUFFICIENT_STORAGE $($args)*) };
    (508 $($args:tt)*) => { $crate::raise!(LOOP_DETECTED $($args)*) };
    (510 $($args:tt)*) => { $crate::raise!(NOT_EXTENDED $($args)*) };
    (511 $($args:tt)*) => { $crate::raise!(NETWORK_AUTHENTICATION_REQUIRED $($args)*) };

    ($status:ident $($args:tt)*) => {
        $crate::raise!(#[ctor] $crate::error::StatusCode::$status $($args)*)
    };

    ($code:literal $($args:tt)*) => {{
        const CODE: u16 = $code;
        const _: () = assert!(
            CODE >= 400 && CODE <= 599,
            "Status code must be in 400..=599 for errors.",
        );

        let Ok(status) = $crate::error::StatusCode::from_u16(CODE) else {
            unreachable!()
        };

        $crate::raise!(#[ctor] status $($args)*)
    }};
}
