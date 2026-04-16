use super::Predicate;

pub struct OneOf {
    values: Vec<Vec<u8>>,
}

macro_rules! cmp_bytes {
    ($($vis:vis fn $ctor:ident($self:ident: &$ty:ident, $rhs:ident: &[u8]) -> bool {
        $matcher:expr
    })+) => {
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
    pub fn case_sensitive(self: &CaseSensitive, value: &[u8]) -> bool {
        self.0.as_slice() == value
    }

    pub fn starts_with(self: &StartsWith, prefix: &[u8]) -> bool {
        prefix.starts_with(self.0.as_slice())
    }

    pub fn ends_with(self: &EndsWith, suffix: &[u8]) -> bool {
        suffix.ends_with(self.0.as_slice())
    }

    pub fn tag(self: &Tag, value: &[u8]) -> bool {
        self.0.as_slice().eq_ignore_ascii_case(value)
    }
}

pub fn one_of<I>(values: I) -> OneOf
where
    I: IntoIterator,
    I::Item: AsRef<[u8]>,
{
    OneOf {
        values: values
            .into_iter()
            .map(|value| value.as_ref().to_owned())
            .collect(),
    }
}

impl Predicate<[u8]> for OneOf {
    type Error<'a> = ();

    fn cmp<'a>(&'a self, input: &[u8]) -> Result<(), Self::Error<'a>> {
        if self
            .values
            .iter()
            .any(|value| value.as_slice().eq_ignore_ascii_case(input))
        {
            Ok(())
        } else {
            Err(())
        }
    }
}
