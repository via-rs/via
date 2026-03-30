use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use via::error::BoxError;

use crate::database::{Id, Identify, Persist};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct User {
    id: Id,
    org_id: Id,
    username: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

#[derive(Debug, Deserialize)]
pub struct NewUser {
    org_id: Option<Id>,
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
            org_id: self.org_id.ok_or("org_id is required.")?,
            username: self.username,
            created_at: now,
            updated_at: now,
        })
    }
}
