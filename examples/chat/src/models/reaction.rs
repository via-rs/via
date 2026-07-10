use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::helper_types::InnerJoin;
use diesel::pg::{Pg, PgValue};
use diesel::serialize::{self, Output, ToSql};
use diesel::sql_types::{Array, BigInt, Integer, Text, VarChar};
use serde::de::{Deserializer, Error as DeError};
use serde::{Deserialize, Serialize, Serializer};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::ops::Deref;
use std::str::FromStr;
use time::OffsetDateTime;
use via_diesel::prelude::*;
use via_diesel::{filters, sorts};

use super::{ThreadWithUser, User, UserPreview};
use crate::app::Connection;
use crate::schema::{reactions, users};
use crate::util::Id;

#[derive(Debug)]
pub struct InvalidEmojiError;

#[derive(AsExpression, Clone, Debug, FromSqlRow)]
#[diesel(sql_type = VarChar)]
pub struct Emoji {
    buf: [u8; 16],
    len: usize,
}

#[derive(Associations, Identifiable, Queryable, Selectable, Serialize)]
#[diesel(belongs_to(ThreadWithUser, foreign_key = thread_id))]
#[diesel(belongs_to(User))]
#[diesel(table_name = reactions)]
#[serde(rename_all = "camelCase")]
pub struct Reaction {
    id: Id,
    emoji: Emoji,

    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,

    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,

    #[serde(skip)]
    thread_id: Id,

    #[serde(skip)]
    user_id: Id,
}

#[derive(AsChangeset, Deserialize)]
#[diesel(table_name = reactions)]
pub struct ChangeSet {
    emoji: Emoji,
}

#[derive(Deserialize, Insertable)]
#[diesel(table_name = reactions)]
#[serde(rename_all = "camelCase")]
pub struct NewReaction {
    pub thread_id: Option<Id>,
    pub user_id: Option<Id>,
    emoji: Emoji,
}

#[derive(Associations, Clone, Deserialize, QueryableByName, Serialize)]
#[diesel(belongs_to(ThreadWithUser, foreign_key = thread_id))]
#[diesel(table_name = reactions)]
#[diesel(check_for_backend(Pg))]
#[serde(rename_all = "camelCase")]
pub struct ReactionPreview {
    emoji: Emoji,

    #[diesel(sql_type = Array<Text>)]
    usernames: Vec<String>,

    #[diesel(sql_type = BigInt)]
    total_count: i64,

    thread_id: Id,
}

#[derive(Queryable, Selectable, Serialize)]
#[diesel(check_for_backend(Pg))]
pub struct ReactionWithUser {
    #[diesel(embed)]
    #[serde(flatten)]
    reaction: Reaction,

    #[diesel(embed)]
    user: UserPreview,
}

filters! {
    pub fn by_id(id == &Id) on reactions;
    pub fn by_user(user_id == &Id) on reactions;
}

sorts! {
    pub fn recent(#[desc] created_at, id) on reactions;
}

impl Reaction {
    pub fn query() -> reactions::table {
        reactions::table
    }

    pub fn with_user() -> InnerJoin<reactions::table, users::table> {
        Self::query().inner_join(users::table)
    }

    pub async fn to_threads<'a>(
        connection: &mut Connection<'_>,
        ids: Vec<Id>,
    ) -> via::Result<Vec<ReactionPreview>> {
        const UNIQUE_REACTIONS_PER_CONVERSATION: i32 = 12;
        const USERNAMES_PER_REACTION: i32 = 6;

        let query = diesel::sql_query("SELECT * FROM top_reactions_for($1, $2, $3)")
            .bind::<Array<BigInt>, Vec<_>>(ids)
            .bind::<Integer, _>(UNIQUE_REACTIONS_PER_CONVERSATION)
            .bind::<Integer, _>(USERNAMES_PER_REACTION);

        AsyncQueryDsl::load(query, connection).await
    }
}

impl ReactionPreview {
    pub fn to_id(&self) -> Id {
        self.thread_id.clone()
    }
}

impl Error for InvalidEmojiError {}

impl Display for InvalidEmojiError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "emoji exceeds max length.")
    }
}

impl FromStr for Emoji {
    type Err = InvalidEmojiError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut emoji = Self {
            buf: [0; 16],
            len: input.len(),
        };

        if emoji.len > 16 {
            Err(InvalidEmojiError)
        } else {
            emoji.buf[..emoji.len].copy_from_slice(input.as_bytes());
            Ok(emoji)
        }
    }
}

impl Deref for Emoji {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        let slice = &self.buf[..self.len];
        // Safety: The bytes in self are guarenteed to be UTF-8.
        unsafe { str::from_utf8_unchecked(slice) }
    }
}

impl<'de> Deserialize<'de> for Emoji {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: &str = Deserialize::deserialize(deserializer)?;
        value.parse().map_err(D::Error::custom)
    }
}

impl FromSql<VarChar, Pg> for Emoji {
    fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
        Ok(str::from_utf8(value.as_bytes())?.parse()?)
    }
}

impl Serialize for Emoji {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self)
    }
}

impl ToSql<VarChar, Pg> for Emoji {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        <str as ToSql<VarChar, Pg>>::to_sql(self, out)
    }
}
