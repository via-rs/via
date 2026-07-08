use bitflags::bitflags;
use diesel::AsExpression;
use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::pg::{Pg, PgValue};
use diesel::prelude::*;
use diesel::serialize::{self, Output, ToSql};
use diesel::sql_types::Integer;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::channel::Channel;
use super::user::UserPreview;
use crate::database::{Id, subscriptions};

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
    subscription: Subscription,

    #[diesel(embed)]
    channel: Channel,
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
    #[diesel(sql_type = Integer)]
    pub struct AuthClaims: i32 {
        const VIEW        = 1 << 0;
        const WRITE       = 1 << 1;
        const REACT       = 1 << 2;
        const INVITE      = 1 << 3;
        const MODERATE    = 1 << 4;
    }
}

filters! {
    pub fn by_id(id == &Id) on subscriptions;
    pub fn by_user(user_id == &Id) on subscriptions;
    pub fn by_channel(channel_id == &Id) on subscriptions;
}

sorts! {
    pub fn recent(#[desc] created_at, id) on subscriptions;
}

diesel::define_sql_function! {
    /// SQL: (lhs & rhs) = rhs
    fn has_flags(lhs: Integer, rhs: Integer) -> Bool;
}

pub fn claims_can_participate() -> has_flags<subscriptions::claims, i32> {
    let participate = AuthClaims::VIEW | AuthClaims::WRITE | AuthClaims::REACT;
    has_flags(subscriptions::claims, participate.bits())
}

impl FromSql<Integer, Pg> for AuthClaims {
    fn from_sql(bytes: PgValue<'_>) -> deserialize::Result<Self> {
        i32::from_sql(bytes).map(Self::from_bits_truncate)
    }
}

impl ToSql<Integer, Pg> for AuthClaims {
    fn to_sql<'a>(&'a self, output: &mut Output<'a, '_, Pg>) -> serialize::Result {
        <_ as ToSql<Integer, _>>::to_sql(&self.bits(), &mut output.reborrow())
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
    pub fn id(&self) -> &Id {
        &self.subscription.id
    }

    // pub fn channel(&self) -> &Channel {
    //     &self.channel
    // }

    pub fn claims(&self) -> &AuthClaims {
        &self.subscription.claims
    }

    pub fn user_id(&self) -> &Id {
        &self.subscription.user_id
    }
}
