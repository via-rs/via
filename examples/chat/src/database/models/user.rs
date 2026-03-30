use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use via::error::BoxError;

use crate::database::{Id, Identify, Persist};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct User {
    id: Id,
    username: String,
    #[serde(with = "time::serde::iso8601")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::iso8601")]
    updated_at: OffsetDateTime,
}

#[derive(Debug, Deserialize)]
pub struct NewUser {
    username: String,
}

impl User {
    pub fn username(&self) -> &str {
        &self.username
    }
}

impl Identify for User {
    fn id(&self) -> &Id {
        &self.id
    }
}

impl Persist for NewUser {
    type Output = User;
    type Error = BoxError;

    fn persist(self, id: Id) -> Result<Self::Output, Self::Error> {
        let now = OffsetDateTime::now_utc();

        Ok(User {
            id,
            username: self.username,
            created_at: now,
            updated_at: now,
        })
    }
}
