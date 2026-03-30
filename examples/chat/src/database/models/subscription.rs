use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use time::UtcDateTime;

use crate::database::{Id, Identify, Persist};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Subscription {
    id: Id,
    channel_id: Id,
    user_id: Id,
    created_at: UtcDateTime,
    updated_at: UtcDateTime,
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
        let now = UtcDateTime::now();

        Ok(Subscription {
            id,
            created_at: now,
            updated_at: now,

            channel_id: self.channel_id,
            user_id: self.user_id,
        })
    }
}
