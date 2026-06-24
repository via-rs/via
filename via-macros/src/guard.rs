/// The client and server agree on a media type and payloads have a known
/// length.
///
/// The first argument is what the client is allowed to send. The second
/// argument is how the server will reply.
#[macro_export]
macro_rules! content {
    ($accepts:expr, $provides:expr) => {
        via::guard::if_else(
            via::guard::method::is_mutation(),
            via::guard::on::headers((
                via::guard::header::accept($provides()),
                via::guard::header::content_type($accepts()),
                via::guard::header::content_length(),
            )),
            via::guard::header::accept($provides()),
        )
    };
    ($accepts:expr) => {
        $crate::content!($accepts, $accepts)
    };
}
