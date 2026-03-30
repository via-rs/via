pub mod session;

pub use session::{Authenticate, Session};

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Body<T> {
    pub data: T,
}

impl<T> Body<T> {
    pub fn new(data: T) -> Self {
        Self { data }
    }

    pub fn data(&self) -> &T {
        &self.data
    }
}
