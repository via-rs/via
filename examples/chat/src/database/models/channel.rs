use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use time::OffsetDateTime;

use crate::database::{Id, Identify, Persist};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    id: Id,
    name: String,
    #[serde(with = "time::serde::iso8601")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::iso8601")]
    updated_at: OffsetDateTime,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewChannel {
    name: String,
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
        let now = OffsetDateTime::now_utc();

        Ok(Channel {
            id,
            name: self.name,
            created_at: now,
            updated_at: now,
        })
    }
}
