use diesel::deserialize::FromSql;
use diesel::expression::{Selectable, SelectableHelper};
use diesel::pg::{Pg, PgValue};
use diesel::{AsExpression, FromSqlRow, deserialize, sql_types};
use serde::{Deserialize, Serialize, Serializer};
use std::sync::Arc;
use time::OffsetDateTime;
use via_diesel::prelude::*;

use super::subscription::NewSubscription;
use super::{ChannelSubscription, ThreadDetails};
use crate::app::Connection;
use crate::schema::{channels, subscriptions};
use crate::util::Id;

#[derive(AsExpression, Clone, Debug, FromSqlRow)]
#[diesel(sql_type = sql_types::Text)]
pub struct Name(Arc<str>);

#[derive(Clone, Identifiable, Queryable, Selectable, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    id: Id,
    name: Option<Name>,

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

    pub async fn destroy(connection: &mut Connection<'_>, id: Id) -> via::Result<usize> {
        diesel::delete(channels::table)
            .filter(by_id(&id))
            .execute(connection)
            .await
    }

    pub async fn update(
        connection: &mut Connection<'_>,
        id: Id,
        changes: ChangeSet,
    ) -> via::Result<Self> {
        diesel::update(channels::table)
            .filter(by_id(&id))
            .set(changes)
            .returning(Self::as_returning())
            .get_result(connection)
            .await
    }
}

impl ChannelWithThreads {
    pub fn new(channel: ChannelSubscription, threads: Vec<ThreadDetails>) -> Self {
        Self { channel, threads }
    }
}

impl FromSql<sql_types::Text, Pg> for Name {
    fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
        let bytes = value.as_bytes();
        let value = str::from_utf8(bytes)?;

        Ok(Self(value.to_owned().into()))
    }
}

impl Serialize for Name {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}
