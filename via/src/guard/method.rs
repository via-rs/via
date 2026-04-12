use super::error::Deny;
use super::predicate::Predicate;
use crate::request::Envelope;

pub struct IsSafe;

pub fn is_safe() -> impl Predicate<Envelope> {
    IsSafe
}

impl Predicate<Envelope> for IsSafe {
    fn matches(&self, envelope: &Envelope) -> Result<(), Deny> {
        if envelope.method().is_safe() {
            Ok(())
        } else {
            Err(Deny::Method)
        }
    }
}
