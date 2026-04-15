use crate::guard::Predicate;

pub struct Contains<T> {
    predicate: T,
}

/// Succeeds if `predicate` matches a comma separated value in the header.
pub fn contains<T>(predicate: T) -> Contains<T> {
    Contains { predicate }
}

impl<T> Predicate<[u8]> for Contains<T>
where
    for<'a> T: Predicate<[u8]> + 'a,
{
    type Error<'a> = ();

    fn cmp<'a>(&'a self, value: &[u8]) -> Result<(), Self::Error<'a>> {
        if value
            .split(|b| *b == b',')
            .any(|item| self.predicate.cmp(item.trim_ascii()).is_ok())
        {
            Ok(())
        } else {
            Err(())
        }
    }
}
