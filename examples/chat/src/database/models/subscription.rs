use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use time::OffsetDateTime;

use crate::database::{Id, Identify, Persist};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    id: Id,
    channel_id: Id,
    user_id: Id,
    #[serde(with = "time::serde::iso8601")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::iso8601")]
    updated_at: OffsetDateTime,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSubscription {
    channel_id: Id,
    user_id: Id,
}

impl Identify for Subscription {
    fn id(&self) -> &Id {
        &self.id
    }
}

impl Persist for NewSubscription {
    type Output = Subscription;
    type Error = Infallible;

    fn persist(self, id: Id) -> Result<Self::Output, Self::Error> {
        let now = OffsetDateTime::now_utc();

        Ok(Subscription {
            id,
            created_at: now,
            updated_at: now,

            channel_id: self.channel_id,
            user_id: self.user_id,
        })
    }
}
