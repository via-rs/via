use super::{Deny, Predicate};

macro_rules! cmp_bytes {
    ($($vis:vis fn $ctor:ident($self:ident: &$ty:ident, $rhs:ident: &[u8]) -> bool {
        $matcher:expr
    })+) => {
        $($vis struct $ty(Box<[u8]>);)+

        $($vis fn $ctor($rhs: &[u8]) -> $ty {
            $ty($rhs.to_owned().into_boxed_slice())
        })+

        $(impl Predicate<[u8]> for $ty {
            fn cmp(&$self, $rhs: &[u8]) -> Result<(), Deny> {
                if $matcher { Ok(()) } else { Err(Deny::Match) }
            }
        })+
    }
}

cmp_bytes! {
    pub fn eq(self: &StrictEq, value: &[u8]) -> bool {
        &*self.0 == value
    }

    pub fn eq_no_case(self: &EqNoCase, value: &[u8]) -> bool {
        (*self.0).eq_ignore_ascii_case(value)
    }

    pub fn starts_with(self: &StartsWith, prefix: &[u8]) -> bool {
        prefix.starts_with(&self.0)
    }

    pub fn ends_with(self: &EndsWith, suffix: &[u8]) -> bool {
        suffix.ends_with(&self.0)
    }
}
