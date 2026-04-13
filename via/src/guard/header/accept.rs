use http::header::ACCEPT;

use super::{Header, header};
use crate::guard::bytes::{StrictEq, eq};
use crate::guard::predicate::{Or, or};

pub fn all() -> Header<StrictEq> {
    header(ACCEPT, eq(b"*/*"))
}

pub fn json() -> Header<Or<(StrictEq, StrictEq, StrictEq)>> {
    header(
        ACCEPT,
        or((
            eq(b"*/*"),
            eq(b"application/json"),
            eq(b"application/json; charset=utf-8"),
        )),
    )
}
