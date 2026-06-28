//! Byte-level predicates for matching header values and URI components.

use super::Predicate;

/// Match a predicate against a comma separated value in the input.
pub struct Contains<T = Tag> {
    predicate: T,
    separator: u8,
}

/// Trim the ASCII whitespace around the input before testing a predicate.
pub struct Trim<T> {
    predicate: T,
}

macro_rules! cmp_bytes {
    ($(
        $(#[$($doc:tt)*])?
        $vis:vis fn $ctor:ident($self:ident: &$ty:ident, $rhs:ident: &[u8]) -> bool {
            $matcher:expr
        }
    )+) => {
        $(
            $(#[$($doc)*])?
            $vis struct $ty(Vec<u8>);

            $(#[$($doc)*])?
            $vis fn $ctor($rhs: &[u8]) -> $ty {
                $ty($rhs.to_owned())
            }

            impl Predicate<[u8]> for $ty {
                type Error<'a> = ();
                fn cmp<'a>(&'a $self, $rhs: &[u8]) -> Result<(), Self::Error<'a>> {
                    if $matcher { Ok(()) } else { Err(()) }
                }
            }
        )+
    }
}

cmp_bytes! {
    #[doc = "The input is equal to or ends with `suffix`."]
    pub fn ends_with(self: &EndsWith, suffix: &[u8]) -> bool {
        suffix.ends_with(self.0.as_slice())
    }

    #[doc = "The input is equal to or starts with `prefix`."]
    pub fn starts_with(self: &StartsWith, prefix: &[u8]) -> bool {
        prefix.starts_with(self.0.as_slice())
    }

    #[doc = "The input is equal to `predicate`."]
    pub fn tag_no_case(self: &TagNoCase, predicate: &[u8]) -> bool {
        self.0.as_slice() == predicate
    }

    #[doc = "The input is a case-insensitive match for `predicate`."]
    pub fn tag(self: &Tag, predicate: &[u8]) -> bool {
        self.0.as_slice().eq_ignore_ascii_case(predicate)
    }
}

/// The `predicate` must match an item in the input separated by `separator`.
///
/// Contains is a short-circuiting predicate that terminates as soon as a
/// matching item is found.
///
/// ASCII whitespace around each input is trimmed before it is tested against
/// `predicate`.
pub fn contains<T>(predicate: T, separator: u8) -> Contains<T> {
    Contains {
        predicate,
        separator,
    }
}

/// Trim the ASCII whitespace around the input before testing `predicate`.
pub fn trim<T>(predicate: T) -> Trim<T> {
    Trim { predicate }
}

impl<T> Predicate<[u8]> for Trim<T>
where
    for<'a> T: Predicate<[u8]> + 'a,
{
    type Error<'a> = T::Error<'a>;

    fn cmp<'a>(&'a self, input: &[u8]) -> Result<(), Self::Error<'a>> {
        self.predicate.cmp(input.trim_ascii())
    }
}

impl<T> Predicate<[u8]> for Contains<T>
where
    for<'a> T: Predicate<[u8]> + 'a,
{
    type Error<'a> = ();

    fn cmp<'a>(&'a self, value: &[u8]) -> Result<(), Self::Error<'a>> {
        if value
            .split(|byte| *byte == self.separator)
            .any(|input| self.predicate.cmp(input).is_ok())
        {
            Ok(())
        } else {
            Err(())
        }
    }
}
