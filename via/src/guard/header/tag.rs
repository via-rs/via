use super::Predicate;
use crate::guard::{ErrorKind, Or, or};

pub type ApplicationJson = Or<(Tag, Tag, Tag)>;

macro_rules! cmp_bytes {
    ($($vis:vis fn $ctor:ident($self:ident: &$ty:ident, $rhs:ident: &[u8]) -> bool {
        $matcher:expr
    })+) => {
        $($vis struct $ty(Box<[u8]>);)+

        $($vis fn $ctor($rhs: &[u8]) -> $ty {
            $ty($rhs.to_owned().into_boxed_slice())
        })+

        $(impl Predicate<[u8]> for $ty {
            fn cmp(&$self, $rhs: &[u8]) -> Result<(), ErrorKind> {
                if $matcher { Ok(()) } else { Err(ErrorKind::Match) }
            }
        })+
    }
}

cmp_bytes! {
    pub fn case_sensitive(self: &CaseSensitive, value: &[u8]) -> bool {
        &*self.0 == value
    }

    pub fn starts_with(self: &StartsWith, prefix: &[u8]) -> bool {
        prefix.starts_with(&self.0)
    }

    pub fn ends_with(self: &EndsWith, suffix: &[u8]) -> bool {
        suffix.ends_with(&self.0)
    }

    pub fn tag(self: &Tag, value: &[u8]) -> bool {
        (*self.0).eq_ignore_ascii_case(value)
    }
}

pub(super) fn application_json() -> ApplicationJson {
    or((
        tag(b"application/json"),
        tag(b"application/json; charset=utf-8"),
        tag(b"application/json;charset=utf-8"),
    ))
}
