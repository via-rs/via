use bitflags::bitflags;
use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::dsl::{AsSelect, InnerJoin, Select};
use diesel::pg::{Pg, PgValue};
use diesel::serialize::{self, Output, ToSql};
use diesel::{AsExpression, sql_types};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use via_diesel::prelude::*;

use super::{Channel, ChannelWithThreads, ThreadDetails, UserPreview};
use crate::schema::{channels, subscriptions, users};
use crate::util::Id;

type JoinChannels = InnerJoin<subscriptions::table, channels::table>;

#[derive(Clone, Debug, Identifiable, Queryable, Selectable, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    id: Id,
    claims: AuthClaims,

    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,

    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,

    #[serde(skip)]
    user_id: Id,
}

#[derive(Clone, Deserialize, Insertable)]
#[diesel(table_name = subscriptions)]
#[serde(rename_all = "camelCase")]
pub struct NewSubscription {
    pub channel_id: Option<Id>,
    pub user_id: Id,
    pub claims: AuthClaims,
}

#[derive(AsChangeset, Deserialize)]
#[diesel(table_name = subscriptions)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSet {
    user_id: Id,
    claims: AuthClaims,
}

#[derive(Clone, Queryable, Selectable, Serialize)]
#[diesel(table_name = subscriptions)]
#[diesel(check_for_backend(Pg))]
pub struct ChannelSubscription {
    #[diesel(embed)]
    #[serde(flatten)]
    channel: Channel,

    #[diesel(embed)]
    subscription: Subscription,
}

#[derive(Queryable, Selectable, Serialize)]
#[diesel(table_name = subscriptions)]
#[diesel(check_for_backend(Pg))]
pub struct UserSubscription {
    #[diesel(embed)]
    #[serde(flatten)]
    subscription: Subscription,

    #[diesel(embed)]
    user: UserPreview,
}

bitflags! {
    #[derive(AsExpression, Clone, Debug, Deserialize, FromSqlRow, Serialize)]
    #[diesel(sql_type = sql_types::Integer)]
    pub struct AuthClaims: i32 {
        const READ         = 1 << 1;
        const WRITE        = 1 << 2;
        const MODERATE     = 1 << 3;
        const ADMINISTRATE = 1 << 4;
    }
}

via_diesel::filters! {
    pub fn by_id(id == &Id) on subscriptions;
    pub fn by_user(user_id == &Id) on subscriptions;
    pub fn by_channel(channel_id == &Id) on subscriptions;
}

via_diesel::sorts! {
    pub fn recent(#[desc] created_at, id) on subscriptions;
}

diesel::define_sql_function! {
    /// SQL: (lhs & rhs) = rhs
    fn has_flags(lhs: sql_types::Integer, rhs: sql_types::Integer) -> sql_types::Bool;
}

pub fn claims_can_participate() -> has_flags<subscriptions::claims, i32> {
    let participate = AuthClaims::READ | AuthClaims::WRITE;
    has_flags(subscriptions::claims, participate.bits())
}

impl FromSql<sql_types::Integer, Pg> for AuthClaims {
    fn from_sql(bytes: PgValue<'_>) -> deserialize::Result<Self> {
        i32::from_sql(bytes).map(Self::from_bits_truncate)
    }
}

impl ToSql<sql_types::Integer, Pg> for AuthClaims {
    fn to_sql<'a>(&'a self, output: &mut Output<'a, '_, Pg>) -> serialize::Result {
        <_ as ToSql<sql_types::Integer, _>>::to_sql(&self.bits(), &mut output.reborrow())
    }
}

impl Subscription {
    pub fn users() -> InnerJoin<subscriptions::table, users::table> {
        subscriptions::table.inner_join(users::table)
    }

    pub fn query() -> Select<subscriptions::table, AsSelect<Self, Pg>> {
        subscriptions::table.select(Self::as_select())
    }
}

impl NewSubscription {
    /// Subscribe user_id to channel_id with all auth claims.
    ///
    pub fn admin(user_id: Id, channel_id: Id) -> Self {
        Self {
            user_id,
            channel_id: Some(channel_id),
            claims: AuthClaims::all(),
        }
    }
}

impl ChannelSubscription {
    pub fn query() -> Select<JoinChannels, AsSelect<Self, Pg>> {
        subscriptions::table
            .inner_join(channels::table)
            .select(Self::as_select())
    }

    pub fn with_threads(self, threads: Vec<ThreadDetails>) -> ChannelWithThreads {
        ChannelWithThreads::new(self, threads)
    }

    pub fn id(&self) -> &Id {
        &self.subscription.id
    }

    pub fn channel(&self) -> &Channel {
        &self.channel
    }

    pub fn claims(&self) -> &AuthClaims {
        &self.subscription.claims
    }

    pub fn user_id(&self) -> &Id {
        &self.subscription.user_id
    }
}
