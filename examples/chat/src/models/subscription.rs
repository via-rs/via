use bitflags::bitflags;
use delegate::delegate;
use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::helper_types::{AsSelect, InnerJoin, Select};
use diesel::pg::{Pg, PgValue};
use diesel::prelude::*;
use diesel::serialize::{self, Output, ToSql};
use diesel::{AsExpression, sql_types};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use via::error::Propagate;
use via::ws;
use via_diesel::AsyncQueryDsl;

use super::Channel;
use crate::app::Connection;
use crate::models::UserPreview;
use crate::schema::{channels, subscriptions};
use crate::util::Id;

type JoinChannels = InnerJoin<subscriptions::table, channels::table>;

#[derive(Clone, Debug, Identifiable, Queryable, Selectable, Serialize)]
#[diesel(check_for_backend(Pg))]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    id: Id,
    claims: AuthClaims,

    #[serde(skip)]
    user_id: Id,

    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,

    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
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

#[derive(Clone, Serialize)]
pub struct AuthorizedSubscription {
    channel: Channel,
    claims: AuthClaims,
    #[serde(skip)]
    keys: SubscriptionKeys,
}

#[derive(Clone, Queryable, Selectable, Serialize)]
#[diesel(table_name = subscriptions)]
pub struct ChannelSubscription {
    #[diesel(embed)]
    #[serde(flatten)]
    channel: Channel,

    #[diesel(embed)]
    subscription: Subscription,
}

#[derive(Queryable, Selectable, Serialize)]
#[diesel(table_name = subscriptions)]
pub struct UserSubscription {
    #[diesel(embed)]
    #[serde(flatten)]
    user: UserPreview,

    #[diesel(embed)]
    subscription: Subscription,
}

#[derive(Clone)]
pub struct SubscriptionKeys {
    channel_id: Id,
    user_id: Id,
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
    pub fn by_id(id == Id) on subscriptions;
    pub fn by_user(user_id == Id) on subscriptions;
    pub fn by_channel(channel_id == &Id) on subscriptions;
}

via_diesel::sorts! {
    pub fn recent(#[desc] created_at, id) on subscriptions;
}

diesel::define_sql_function! {
    /// SQL: (lhs & rhs) = rhs
    fn has_flags(lhs: sql_types::Integer, rhs: sql_types::Integer) -> sql_types::Bool;
}

pub fn can_participate() -> has_flags<subscriptions::claims, i32> {
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
    pub async fn participating(connection: &mut Connection<'_>, user: Id) -> ws::Result<Vec<Id>> {
        let query = subscriptions::table
            .inner_join(channels::table)
            .select(channels::id)
            .filter(by_user(user).and(can_participate()))
            .limit(1000);

        query.load_async(connection).await.or_break()
    }

    pub fn query() -> Select<JoinChannels, AsSelect<Self, Pg>> {
        subscriptions::table
            .inner_join(channels::table)
            .select(Self::as_select())
    }
}

impl AuthorizedSubscription {
    pub async fn fetch(
        connection: &mut Connection<'_>,
        keys: &SubscriptionKeys,
    ) -> via::Result<Self> {
        // The user in `keys` must have the minimum auth claims required to
        // participate in the channel.
        let by_parciptiation = subscriptions::channel_id
            .eq(keys.channel_id)
            .and(subscriptions::user_id.eq(keys.user_id))
            .and(can_participate());

        // Execute the query.
        let parcipitating = ChannelSubscription::query()
            .filter(by_parciptiation)
            .fetch_first_async(connection)
            .await?;

        // Rebuild the subscription keys with the values provided by diesel.
        let keys = SubscriptionKeys {
            channel_id: parcipitating.channel.id,
            user_id: parcipitating.subscription.user_id,
        };

        Ok(Self {
            channel: parcipitating.channel,
            claims: parcipitating.subscription.claims,
            keys,
        })
    }

    delegate! {
        to self.keys() {
            pub fn channel_id(&self) -> &Id;
            pub fn user_id(&self) -> &Id;
        }
    }

    pub fn is_admin(&self) -> bool {
        self.claims.contains(AuthClaims::ADMINISTRATE)
    }

    pub fn to_keys(&self) -> SubscriptionKeys {
        SubscriptionKeys {
            channel_id: *self.channel_id(),
            user_id: *self.user_id(),
        }
    }

    fn keys(&self) -> &SubscriptionKeys {
        &self.keys
    }
}

impl SubscriptionKeys {
    pub fn new(user_id: Id, channel_id: Id) -> Self {
        Self {
            channel_id,
            user_id,
        }
    }

    pub fn channel_id(&self) -> &Id {
        &self.channel_id
    }

    pub fn user_id(&self) -> &Id {
        &self.user_id
    }
}
