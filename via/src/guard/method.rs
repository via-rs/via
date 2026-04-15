use super::predicate::{Not, Predicate, not};
use crate::request::Request;

pub struct IsSafe;

pub fn is_mutation() -> Not<IsSafe> {
    not(is_safe())
}

pub fn is_safe() -> IsSafe {
    IsSafe
}

impl<App> Predicate<Request<App>> for IsSafe {
    type Error<'a> = ();

    fn cmp<'a>(&'a self, request: &Request<App>) -> Result<(), Self::Error<'a>> {
        if request.method().is_safe() {
            Ok(())
        } else {
            Err(())
        }
    }
}
