use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use time::UtcDateTime;

use crate::database::{Id, Identify, Persist};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    id: Id,
    name: String,
    org_id: Id,
    created_at: UtcDateTime,
    updated_at: UtcDateTime,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewChannel {
    name: String,
    org_id: Id,
}

impl Identify for Channel {
    fn id(&self) -> &Id {
        &self.id
    }
}

impl Persist for NewChannel {
    type Output = Channel;
    type Error = Infallible;

    fn persist(self, id: Id) -> Result<Self::Output, Self::Error> {
        let now = UtcDateTime::now();

        Ok(Channel {
            id,
            name: self.name,
            org_id: self.org_id,
            created_at: now,
            updated_at: now,
        })
    }
}
