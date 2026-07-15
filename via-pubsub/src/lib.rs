pub mod backend;

mod opaque;
mod pubsub;

pub use backend::{Event, Interest, Publish};
pub use opaque::Opaque;
pub use pubsub::{Pubsub, Subscription};

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
