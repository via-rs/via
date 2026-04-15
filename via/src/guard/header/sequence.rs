use crate::guard::{GuardError, Predicate};

pub struct Contains<T>(Has<Comma, T>);

struct Comma;

struct Has<T, U> {
    separator: T,
    predicate: U,
}

/// Succeeds if `predicate` matches a comma separated value in the header.
pub fn contains<T>(predicate: T) -> Contains<T> {
    Contains(Has {
        separator: Comma,
        predicate,
    })
}

impl Predicate<u8> for Comma {
    fn cmp<'a>(&'a self, byte: &u8) -> Result<(), GuardError<'a>> {
        if *byte == b',' {
            Ok(())
        } else {
            Err(GuardError::Match)
        }
    }
}

impl<T> Predicate<[u8]> for Contains<T>
where
    T: Predicate<[u8]>,
{
    fn cmp<'a>(&'a self, value: &[u8]) -> Result<(), GuardError<'a>> {
        self.0.cmp(value)
    }
}

impl<T, U> Predicate<[u8]> for Has<T, U>
where
    T: Predicate<u8>,
    U: Predicate<[u8]>,
{
    fn cmp<'a>(&'a self, value: &[u8]) -> Result<(), GuardError<'a>> {
        if value
            .split(|byte| self.separator.cmp(byte).is_ok())
            .any(|item| self.predicate.cmp(item.trim_ascii()).is_ok())
        {
            Ok(())
        } else {
            Err(GuardError::Match)
        }
    }
}
