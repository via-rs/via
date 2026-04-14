use crate::guard::{ErrorKind, Not, Predicate, not};

pub struct Contains<T>(Has<Comma, T>);

pub struct Token<T>(Has<Not<Tchar>, T>);

struct Comma;

struct Tchar;

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

/// Succeeds if `predicate` matches one of the tokens in the header value.
pub fn token<T>(predicate: T) -> Token<T> {
    Token(Has {
        separator: not(Tchar),
        predicate,
    })
}

impl Predicate<u8> for Comma {
    fn cmp(&self, byte: &u8) -> Result<(), ErrorKind> {
        if *byte == b',' {
            Ok(())
        } else {
            Err(ErrorKind::Match)
        }
    }
}

impl Predicate<u8> for Tchar {
    fn cmp(&self, byte: &u8) -> Result<(), ErrorKind> {
        match byte {
            0x21
            | 0x23..=0x27
            | 0x2A
            | 0x2B
            | 0x2D
            | 0x2E
            | 0x30..=0x39
            | 0x41..=0x5A
            | 0x5E..=0x7A
            | 0x7C
            | 0x7E => Ok(()),
            _ => Err(ErrorKind::Match),
        }
    }
}

impl<T> Predicate<[u8]> for Contains<T>
where
    T: Predicate<[u8]>,
{
    fn cmp(&self, value: &[u8]) -> Result<(), ErrorKind> {
        self.0.cmp(value)
    }
}

impl<T, U> Predicate<[u8]> for Has<T, U>
where
    T: Predicate<u8>,
    U: Predicate<[u8]>,
{
    fn cmp(&self, value: &[u8]) -> Result<(), ErrorKind> {
        if value
            .split(|byte| self.separator.matches(byte))
            .any(|item| self.predicate.matches(item.trim_ascii()))
        {
            Ok(())
        } else {
            Err(ErrorKind::Match)
        }
    }
}
