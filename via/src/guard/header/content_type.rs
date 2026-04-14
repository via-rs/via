use http::header::CONTENT_TYPE;

use super::tag::{ApplicationJson, application_json};
use super::{Header, header};

pub fn content_type<T>(predicate: T) -> Header<T> {
    header(CONTENT_TYPE, predicate)
}

pub fn json() -> Header<ApplicationJson> {
    content_type(application_json())
}
