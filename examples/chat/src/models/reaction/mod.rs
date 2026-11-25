mod emoji;
mod params;

pub use emoji::Emoji;
pub use params::{ReactionCollectionContext, ReactionMemberContext, WithReactedToId};

use diesel::dsl::{AsSelect, Select};
use diesel::helper_types::InnerJoin;
use diesel::prelude::*;
use diesel::{pg::Pg, sql_types};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use via_diesel::{AsyncQueryDsl, LimitAndPage, Paginate};

use super::{ThreadWithUser, User, UserPreview};
use crate::app::Connection;
use crate::models::ThreadDetails;
use crate::schema::{reactions, threads, users};
use crate::util::Id;

const UNIQUE_REACTIONS_PER_CONVERSATION: i32 = 12;
const USERNAMES_PER_REACTION: i32 = 6;

type JoinUsers = InnerJoin<reactions::table, users::table>;
type JoinUsersAndThreads = InnerJoin<JoinUsers, threads::table>;

#[derive(
    Associations,
    Debug,
    Deserialize,
    Identifiable,
    Queryable,
    QueryableByName,
    Selectable,
    Serialize,
)]
#[diesel(belongs_to(User))]
#[serde(rename_all = "camelCase")]
pub struct Reaction {
    id: Id,
    emoji: Emoji,

    thread_id: Id,
    user_id: Id,

    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,

    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
}

#[derive(AsChangeset, Deserialize)]
#[diesel(table_name = reactions)]
pub struct ChangeSet {
    emoji: Emoji,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewReaction {
    emoji: Emoji,
}

#[derive(Debug, Deserialize, Insertable)]
#[diesel(table_name = reactions)]
#[serde(rename_all = "camelCase")]
pub struct NewReactionInChannel {
    #[diesel(skip_insertion)]
    pub channel_id: Id,
    pub user_id: Option<Id>,

    emoji: Emoji,

    thread_id: Id,
}

#[derive(Associations, Clone, Deserialize, QueryableByName, Serialize)]
#[diesel(belongs_to(ThreadWithUser, foreign_key = thread_id))]
#[diesel(table_name = reactions)]
#[diesel(check_for_backend(Pg))]
#[serde(rename_all = "camelCase")]
pub struct ReactionPreview {
    emoji: Emoji,

    #[diesel(sql_type = sql_types::Array<sql_types::Text>)]
    usernames: Vec<String>,

    #[diesel(sql_type = sql_types::BigInt)]
    total_count: i64,

    thread_id: Id,
}

#[derive(Debug, Deserialize, Queryable, QueryableByName, Selectable, Serialize)]
#[diesel(check_for_backend(Pg), table_name = reactions)]
pub struct ReactionWithUser {
    #[diesel(embed)]
    #[serde(flatten)]
    reaction: Reaction,

    #[diesel(embed)]
    user: UserPreview,
}

via_diesel::filters! {
    pub fn by_id(id == Id) on reactions;
    pub fn by_user(user_id == Id) on reactions;
}

via_diesel::sorts! {
    pub fn recent(#[desc] created_at, id) on reactions;
}

impl Reaction {
    pub async fn create(
        connection: &mut Connection<'_>,
        context: &ReactionCollectionContext,
        args: NewReaction,
    ) -> via::Result<ReactionWithUser> {
        diesel::sql_query("SELECT * FROM react_to($1, $2, $3)")
            .bind::<sql_types::Uuid, _>(context.reacted_to_id())
            .bind::<sql_types::Nullable<sql_types::Uuid>, _>(context.user_id())
            .bind::<sql_types::VarChar, _>(&args.emoji)
            .get_result_async(connection)
            .await
    }

    pub fn query() -> reactions::table {
        reactions::table
    }

    pub async fn to_thread(
        connection: &mut Connection<'_>,
        id: &Id,
    ) -> via::Result<Vec<ReactionPreview>> {
        diesel::sql_query("SELECT * FROM top_reactions_for($1, $2, $3)")
            .bind::<sql_types::Array<sql_types::Uuid>, _>(vec![id])
            .bind::<sql_types::Integer, _>(UNIQUE_REACTIONS_PER_CONVERSATION)
            .bind::<sql_types::Integer, _>(USERNAMES_PER_REACTION)
            .load_async(connection)
            .await
    }

    pub async fn to_threads(
        connection: &mut Connection<'_>,
        threads: Vec<ThreadWithUser>,
    ) -> via::Result<Vec<ThreadDetails>> {
        let thread_ids = threads.iter().map(|thread| thread.id()).collect();
        let reactions = diesel::sql_query("SELECT * FROM top_reactions_for($1, $2, $3)")
            .bind::<sql_types::Array<sql_types::Uuid>, Vec<&Id>>(thread_ids)
            .bind::<sql_types::Integer, _>(UNIQUE_REACTIONS_PER_CONVERSATION)
            .bind::<sql_types::Integer, _>(USERNAMES_PER_REACTION)
            .load_async(connection)
            .await?;

        Ok(ThreadDetails::grouped_by(threads, reactions))
    }
}

impl ReactionWithUser {
    pub fn find_by_context(
        connection: &mut Connection<'_>,
        context: &ReactionMemberContext,
    ) -> impl Future<Output = via::Result<Self>> {
        let by_channel_thread_and_id = reactions::id
            .eq(context.reaction_id())
            .and(threads::channel_id.eq(context.channel_id()))
            .and(reactions::thread_id.eq(context.reacted_to_id()));

        Self::query()
            .filter(by_channel_thread_and_id)
            .first_async(connection)
    }

    pub fn page_by_context(
        connection: &mut Connection<'_>,
        context: &ReactionCollectionContext,
        limit_and_page: LimitAndPage<50>,
    ) -> impl Future<Output = via::Result<Vec<Self>>> {
        let by_channel_and_thread = threads::channel_id
            .eq(context.channel_id())
            .and(reactions::thread_id.eq(context.reacted_to_id()));

        Self::query()
            .filter(by_channel_and_thread)
            .page(limit_and_page)
            .load_async(connection)
    }

    fn query() -> Select<JoinUsersAndThreads, AsSelect<Self, Pg>> {
        reactions::table
            .inner_join(users::table)
            .inner_join(threads::table)
            .select(Self::as_select())
    }
}
