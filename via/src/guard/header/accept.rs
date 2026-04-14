use http::header::ACCEPT;

use super::sequence::{Contains, contains};
use super::tag::{self, ApplicationJson, CaseSensitive, case_sensitive};
use super::{Header, header};
use crate::guard::predicate::{Or, or};

pub type Accept<T> = Contains<Or<(CaseSensitive, T)>>;

pub fn accept<T>(predicate: T) -> Header<Accept<T>> {
    header(ACCEPT, contains(or((case_sensitive(b"*/*"), predicate))))
}

pub fn json() -> Header<Accept<ApplicationJson>> {
    accept(tag::application_json())
}
