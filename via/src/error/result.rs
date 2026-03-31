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
            .map_err(|error| set_status(error, StatusCode::PROXY_AUTHENTICATION_REQUIRED))
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
        self.into_result()
            .map_err(|error| set_status(error, StatusCode::GONE))
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
            .map_err(|error| set_status(error, StatusCode::UNSUPPORTED_MEDIA_TYPE))
    }

    fn or_range_not_satisfiable(self) -> Result<Self::Output, Error> {
        self.into_result()
            .map_err(|error| set_status(error, StatusCode::RANGE_NOT_SATISFIABLE))
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
        self.into_result()
            .map_err(|error| set_status(error, StatusCode::LOCKED))
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
            .map_err(|error| set_status(error, StatusCode::PRECONDITION_REQUIRED))
    }

    fn or_too_many_requests(self) -> Result<Self::Output, Error> {
        self.into_result()
            .map_err(|error| set_status(error, StatusCode::TOO_MANY_REQUESTS))
    }

    fn or_request_header_fields_too_large(self) -> Result<Self::Output, Error> {
        self.into_result()
            .map_err(|error| set_status(error, StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE))
    }

    fn or_unavailable_for_legal_reasons(self) -> Result<Self::Output, Error> {
        self.into_result()
            .map_err(|error| set_status(error, StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS))
    }

    fn or_internal_server_error(self) -> Result<Self::Output, Error> {
        self.into_result()
            .map_err(|error| set_status(error, StatusCode::INTERNAL_SERVER_ERROR))
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
            .map_err(|error| set_status(error, StatusCode::HTTP_VERSION_NOT_SUPPORTED))
    }

    fn or_variant_also_negotiates(self) -> Result<Self::Output, Error> {
        self.into_result()
            .map_err(|error| set_status(error, StatusCode::VARIANT_ALSO_NEGOTIATES))
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
            .map_err(|error| set_status(error, StatusCode::NETWORK_AUTHENTICATION_REQUIRED))
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
