/// Negotiate request and response media types.
///
/// `content!` builds a predicate that validates the request's content
/// contract before body parsing or response generation occurs.
///
/// The first argument describes what the client is allowed to send. The second
/// argument describes what the server will send in response.
///
/// ```
/// # use via::guard::{self, media};
/// let predicate = guard::content!(media::json(), media::json());
/// ```
///
/// If both sides use the same media predicate, pass a single argument:
///
/// ```
/// # use via::guard::{self, media};
/// let predicate = guard::content!(media::json());
/// ```
///
/// For safe requests, only the `Accept` header is checked. Safe requests do
/// not carry a required request payload, so `Content-Type` and
/// `Content-Length` are not required.
///
/// For mutation requests, `content!` checks all of the following:
///
/// - `Accept` must include the response media type provided by the server.
/// - `Content-Type` must match the request media type accepted by the server.
/// - `Content-Length` must be present.
///
/// This makes `content!` useful as a subtree `barrier`. If a request cannot
/// exchange the expected media type, downstream middleware such as session
/// restoration, body aggregation, and deserialization can be skipped.
///
/// # Example
///
/// ```
/// # use via::guard::{self, media};
/// # let mut app = via::app(());
/// app.middleware(guard::barrier(guard::content!(media::json())));
/// ```
#[macro_export]
macro_rules! content {
    ($accepts:expr, $provides:expr) => {
        via::guard::if_else(
            via::guard::method::is_mutation(),
            via::guard::on::headers((
                via::guard::header::accept($provides),
                via::guard::header::content_type($accepts),
                via::guard::header::content_length(),
            )),
            via::guard::on::headers(via::guard::header::accept($provides)),
        )
    };
    ($accepts:expr) => {
        $crate::content!($accepts, $accepts)
    };
}
