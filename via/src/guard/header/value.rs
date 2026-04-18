use super::Predicate;

/// Match a predicate against a comma separated value in the input.
pub struct Contains<T = Tag>(T);

macro_rules! cmp_bytes {
    ($(
        $(#[$doc:meta])?
        $vis:vis fn $ctor:ident($self:ident: &$ty:ident, $rhs:ident: &[u8]) -> bool {
            $matcher:expr
        }
    )+) => {
        $($vis struct $ty(Vec<u8>);)+
        $($vis fn $ctor($rhs: &[u8]) -> $ty { $ty($rhs.to_owned()) })+
        $(impl Predicate<[u8]> for $ty {
            type Error<'a> = ();
            fn cmp<'a>(&'a $self, $rhs: &[u8]) -> Result<(), Self::Error<'a>> {
                if $matcher { Ok(()) } else { Err(()) }
            }
        })+
    }
}

cmp_bytes! {
    #[doc = "The input is equal to `predicate`."]
    pub fn case_sensitive(self: &CaseSensitive, predicate: &[u8]) -> bool {
        self.0.as_slice() == predicate
    }

    #[doc = "The input is equal to or starts with `prefix`."]
    pub fn starts_with(self: &StartsWith, prefix: &[u8]) -> bool {
        prefix.starts_with(self.0.as_slice())
    }

    #[doc = "The input is equal to or ends with `suffix`."]
    pub fn ends_with(self: &EndsWith, suffix: &[u8]) -> bool {
        suffix.ends_with(self.0.as_slice())
    }

    #[doc = "The input is a case-insensitive match for `predicate`."]
    pub fn tag(self: &Tag, predicate: &[u8]) -> bool {
        self.0.as_slice().eq_ignore_ascii_case(predicate)
    }
}

/// Match `predicate` against a comma separated value in the input.
pub fn contains<T>(predicate: T) -> Contains<T> {
    Contains(predicate)
}

impl<T> Predicate<[u8]> for Contains<T>
where
    for<'a> T: Predicate<[u8]> + 'a,
{
    type Error<'a> = ();

    fn cmp<'a>(&'a self, value: &[u8]) -> Result<(), Self::Error<'a>> {
        if value
            .split(|b| *b == b',')
            .any(|item| self.0.cmp(item.trim_ascii()).is_ok())
        {
            Ok(())
        } else {
            Err(())
        }
    }
}
