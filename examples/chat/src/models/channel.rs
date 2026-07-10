use diesel::expression::{Selectable, SelectableHelper};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use via_diesel::prelude::*;

use super::subscription::{ChannelSubscription, NewSubscription};
use super::{ThreadDetails, UserPreview};
use crate::app::Connection;
use crate::schema::{channels, subscriptions};
use crate::util::Id;

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
pub struct ChannelWithThreads {
    #[serde(flatten)]
    channel: ChannelSubscription,
    threads: Vec<ThreadDetails>,
}

via_diesel::filters! {
    pub fn by_id(id == &Id) on channels;
}

impl Channel {
    pub fn create<'a>(
        connection: &'a mut Connection,
        owner_id: Id,
        init: NewChannel,
    ) -> impl Future<Output = via::Result<Self>> + 'a {
        use diesel_async::AsyncConnection;

        connection.transaction(async move |trx| {
            // Insert the channel into the channels table.
            let channel = diesel::insert_into(channels::table)
                .values(init)
                .returning(Channel::as_returning())
                .get_result(trx)
                .await?;

            // Associate the active user to the channel as an admin.
            diesel::insert_into(subscriptions::table)
                .values(NewSubscription::admin(owner_id, *channel.id()))
                .execute(trx)
                .await?;

            Ok(channel)
        })
    }
}

impl ChannelWithThreads {
    pub fn new(channel: ChannelSubscription, threads: Vec<ThreadDetails>) -> Self {
        Self { channel, threads }
    }
}
