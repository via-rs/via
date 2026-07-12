use super::Error;

macro_rules! or_status {
    ($(
        #[code = $code:literal]
        fn $name:ident($self:ident) -> $status:ident;
    )*) => {
        $(
            #[doc = concat!(
                "Converts self into a result and maps the [`Err`] variant to ",
                "an [`Error`] with a ", "[", $code, "] status code.",
                "\n\n",
                "[", $code, "]:",
                "https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Status/", $code
            )]
            fn $name($self) -> Result<Self::Output, Error> {
                $self.into_result().map_err(|mut error| {
                    error.status = http::StatusCode::$status;
                    error
                })
            }
        )*
    };
}

/// An extension trait that converts result-like data structures into a
/// [`Result`](crate::Result).
///
/// # Example
///
/// ```
/// use via::{Next, Request, Response, ResultExt};
///
/// async fn hello(request: Request, _: Next) -> via::Result {
///     // Get a reference to `name` from the request uri path.
///     let name = request.param("name").or_bad_request()?;
///
///     // Send a plain text response with our greeting message.
///     Response::build().text(format!("Hello, {}!", name.as_ref()))
/// }
/// ```
pub trait ResultExt: Sized {
    /// The type returned in the [`Ok`] variant when self is converted into a
    /// result.
    type Output;

    /// Converts self into a result.
    fn into_result(self) -> Result<Self::Output, Error>;

    or_status! {
        #[code = 400]
        fn or_bad_request(self) -> BAD_REQUEST;

        #[code = 401]
        fn or_unauthorized(self) -> UNAUTHORIZED;

        #[code = 402]
        fn or_payment_required(self) -> PAYMENT_REQUIRED;

        #[code = 403]
        fn or_forbidden(self) -> FORBIDDEN;

        #[code = 404]
        fn or_not_found(self) -> NOT_FOUND;

        #[code = 405]
        fn or_method_not_allowed(self) -> METHOD_NOT_ALLOWED;

        #[code = 406]
        fn or_not_acceptable(self) -> NOT_ACCEPTABLE;

        #[code = 407]
        fn or_proxy_authentication_required(self) -> PROXY_AUTHENTICATION_REQUIRED;

        #[code = 408]
        fn or_request_timeout(self) -> REQUEST_TIMEOUT;

        #[code = 409]
        fn or_conflict(self) -> CONFLICT;

        #[code = 410]
        fn or_gone(self) -> GONE;

        #[code = 411]
        fn or_length_required(self) -> LENGTH_REQUIRED;

        #[code = 412]
        fn or_precondition_failed(self) -> PRECONDITION_FAILED;

        #[code = 413]
        fn or_payload_too_large(self) -> PAYLOAD_TOO_LARGE;

        #[code = 414]
        fn or_uri_too_long(self) -> URI_TOO_LONG;

        #[code = 415]
        fn or_unsupported_media_type(self) -> UNSUPPORTED_MEDIA_TYPE;

        #[code = 416]
        fn or_range_not_satisfiable(self) -> RANGE_NOT_SATISFIABLE;

        #[code = 417]
        fn or_expectation_failed(self) -> EXPECTATION_FAILED;

        #[code = 418]
        fn or_im_a_teapot(self) -> IM_A_TEAPOT;

        #[code = 421]
        fn or_misdirected_request(self) -> MISDIRECTED_REQUEST;

        #[code = 422]
        fn or_unprocessable_entity(self) -> UNPROCESSABLE_ENTITY;

        #[code = 423]
        fn or_locked(self) -> LOCKED;

        #[code = 424]
        fn or_failed_dependency(self) -> FAILED_DEPENDENCY;

        #[code = 426]
        fn or_upgrade_required(self) -> UPGRADE_REQUIRED;

        #[code = 428]
        fn or_precondition_required(self) -> PRECONDITION_REQUIRED;

        #[code = 429]
        fn or_too_many_requests(self) -> TOO_MANY_REQUESTS;

        #[code = 431]
        fn or_request_header_fields_too_large(self) -> REQUEST_HEADER_FIELDS_TOO_LARGE;

        #[code = 451]
        fn or_unavailable_for_legal_reasons(self) -> UNAVAILABLE_FOR_LEGAL_REASONS;

        #[code = 500]
        fn or_internal_server_error(self) -> INTERNAL_SERVER_ERROR;

        #[code = 501]
        fn or_not_implemented(self) -> NOT_IMPLEMENTED;

        #[code = 502]
        fn or_bad_gateway(self) -> BAD_GATEWAY;

        #[code = 503]
        fn or_service_unavailable(self) -> SERVICE_UNAVAILABLE;

        #[code = 504]
        fn or_gateway_timeout(self) -> GATEWAY_TIMEOUT;

        #[code = 505]
        fn or_http_version_not_supported(self) -> HTTP_VERSION_NOT_SUPPORTED;

        #[code = 506]
        fn or_variant_also_negotiates(self) -> VARIANT_ALSO_NEGOTIATES;

        #[code = 507]
        fn or_insufficient_storage(self) -> INSUFFICIENT_STORAGE;

        #[code = 508]
        fn or_loop_detected(self) -> LOOP_DETECTED;

        #[code = 510]
        fn or_not_extended(self) -> NOT_EXTENDED;

        #[code = 511]
        fn or_network_authentication_required(self) -> NETWORK_AUTHENTICATION_REQUIRED;
    }
}

impl<T> ResultExt for Option<T> {
    type Output = T;

    #[inline]
    fn into_result(self) -> Result<Self::Output, Error> {
        self.or_not_found()
    }

    #[inline]
    fn or_not_found(self) -> Result<Self::Output, Error> {
        self.ok_or_else(|| crate::err!(404, "not found"))
    }
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
