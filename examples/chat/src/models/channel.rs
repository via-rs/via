use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::thread::ThreadDetails;
use super::user::UserPreview;
use crate::database::{Id, channels};

#[derive(Clone, Identifiable, Queryable, Selectable, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    id: Id,
    name: Option<String>,

    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,

    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
}

#[derive(Clone, Deserialize, Insertable)]
#[diesel(table_name = channels)]
pub struct NewChannel {
    name: Option<String>,
}

#[derive(AsChangeset, Deserialize)]
#[diesel(table_name = channels)]
pub struct ChangeSet {
    name: String,
}

#[derive(Serialize)]
pub struct ChannelWithJoins {
    #[serde(flatten)]
    channel: Channel,
    users: Vec<UserPreview>,
    threads: Vec<ThreadDetails>,
}

filters! {
    pub fn by_id(id == &Id) on channels;
}
