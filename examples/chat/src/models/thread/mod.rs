mod params;

pub use params::{ThreadCollectionContext, ThreadMemberContext};

use diesel::associations::HasTable;
use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::helper_types::{AsSelect, InnerJoin, Select};
use diesel::pg::Pg;
use diesel::{prelude::*, sql_types};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use via::ResultExt;
use via::request::PathParams;
use via_diesel::paginate::{Keyset, PER_PAGE};
use via_diesel::{AsyncQueryDsl, Paginate};

use super::subscription::{ChannelSubscription, Subscription};
use super::user::{User, UserPreview, UserPreviewSqlType};
use super::{Channel, Reaction, ReactionPreview};
use crate::app::Connection;
use crate::schema::{threads, users};
use crate::util::Id;

pub type JoinUsers = InnerJoin<threads::table, users::table>;
pub type SelectThreadWithUser = Select<JoinUsers, AsSelect<ThreadWithUser, Pg>>;

type ThreadSqlType = (
    sql_types::Uuid,                      // id
    sql_types::Text,                      // body
    sql_types::Uuid,                      // channel_id
    sql_types::Nullable<sql_types::Uuid>, // thread_id
    sql_types::Uuid,                      // user_id
    sql_types::Timestamptz,               // created_at
    sql_types::Timestamptz,               // updated_at
    sql_types::BigInt,                    // total_reactions
    sql_types::BigInt,                    // total_replies
);

type ReplyToResult = (
    sql_types::Uuid,                      // id
    sql_types::Text,                      // body
    sql_types::Uuid,                      // channel_id
    sql_types::Nullable<sql_types::Uuid>, // thread_id
    sql_types::Uuid,                      // user_id
    sql_types::Timestamptz,               // created_at
    sql_types::Timestamptz,               // updated_at
    sql_types::BigInt,                    // total_reactions
    sql_types::BigInt,                    // total_replies
    sql_types::Uuid,                      // id
    sql_types::Uuid,                      // id
    sql_types::Text,                      // body
);

#[derive(
    Associations,
    Clone,
    Debug,
    Deserialize,
    Identifiable,
    Queryable,
    QueryableByName,
    Selectable,
    Serialize,
)]
#[diesel(
    belongs_to(Channel),
    belongs_to(Thread),
    belongs_to(User),
    check_for_backend(Pg)
)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    id: Id,
    body: String,

    channel_id: Id,

    #[serde(skip_serializing_if = "Option::is_none")]
    thread_id: Option<Id>,

    user_id: Id,

    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,

    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,

    total_reactions: i64,
    total_replies: i64,
}

#[derive(Debug, Deserialize)]
pub struct PutThread {
    body: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadDelta {
    id: Id,
    body: String,
    user: UserPreview,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
}

#[derive(Deserialize, Serialize)]
pub struct ThreadDetails {
    #[serde(flatten)]
    thread: Thread,

    user: UserPreview,

    reactions: Vec<ReactionPreview>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    replies: Vec<ThreadDetails>,
}

#[derive(Debug, Deserialize, Queryable, QueryableByName, Selectable, Serialize)]
#[diesel(table_name = threads)]
pub struct ThreadWithUser {
    #[diesel(embed)]
    #[serde(flatten)]
    thread: Thread,

    #[diesel(embed)]
    user: UserPreview,
}

#[derive(Debug, QueryableByName, Serialize)]
pub struct UpdatedThread {
    #[diesel(sql_type = sql_types::Bool)]
    #[serde(skip)]
    did_change: bool,

    #[diesel(embed)]
    #[serde(flatten)]
    thread: ThreadWithUser,
}

via_diesel::sorts! {
    pub fn recent(#[desc] created_at, #[desc] id) on threads;
}

impl Thread {
    pub async fn create(
        connection: &mut Connection<'_>,
        context: &ThreadCollectionContext,
        new_thread: PutThread,
    ) -> via::Result<ThreadWithUser> {
        // We use raw SQL to eagerly load the profile view of the author.
        diesel::sql_query("SELECT * FROM reply_to($1, $2, $3, $4)")
            .bind::<sql_types::Uuid, _>(context.channel_id())
            .bind::<sql_types::Nullable<sql_types::Uuid>, _>(context.thread_id())
            .bind::<sql_types::Uuid, _>(context.user_id())
            .bind::<sql_types::Text, _>(new_thread.body)
            .get_result_async(connection)
            .await
    }

    pub async fn update(
        connection: &mut Connection<'_>,
        context: &ThreadMemberContext,
        change_set: PutThread,
    ) -> via::Result<UpdatedThread> {
        let mut parent_id = None;
        let own_id = if let Some(id) = context.reply_id() {
            parent_id = Some(context.thread_id());
            id
        } else {
            context.thread_id()
        };

        diesel::sql_query("SELECT * FROM update_thread($1, $2, $3, $4)")
            .bind::<sql_types::Uuid, _>(own_id)
            .bind::<sql_types::Uuid, _>(context.user_id())
            .bind::<sql_types::Nullable<sql_types::Uuid>, _>(parent_id)
            .bind::<sql_types::Text, _>(change_set.body)
            .get_result_async(connection)
            .await
    }
}

impl ThreadDetails {
    pub async fn find_thread(
        connection: &mut Connection<'_>,
        id: &Id,
    ) -> via::Result<ThreadDetails> {
        let target = diesel::alias!(threads as target);

        let target_id = target.field(threads::id);
        let target_thread_id = target.field(threads::thread_id);

        let belongs_to_target_root = target_thread_id
            .is_null()
            .and(
                threads::id
                    .eq(target_id)
                    .or(threads::thread_id.eq(target_id.nullable())),
            )
            .or(target_thread_id.is_not_null().and(
                threads::id
                    .nullable()
                    .eq(target_thread_id)
                    .or(threads::thread_id.eq(target_thread_id)),
            ));

        // Load the thread by id along with the first page of replies.
        let mut replies = {
            let threads = ThreadWithUser::query()
                .inner_join(target.on(target_id.eq(id).and(belongs_to_target_root)))
                .filter(threads::id.eq(id).and(threads::thread_id.is_null()))
                .or_filter(threads::thread_id.eq(id))
                .order(recent())
                .limit(PER_PAGE + 1)
                .load_async(connection)
                .await?;

            // Side load the reactions to the threads in `threads`.
            // Interweave the reactions in the thread to which they belong.
            Reaction::to_threads(connection, threads).await?
        };

        // Pop the parent thread from `replies`.
        // They are ordered by recent. Therefore, the parent is always last.
        let mut thread = replies.pop().or_not_found()?;

        // Reverse the replies to match their render sequence.
        replies.reverse();

        // Set the replies field of `thread` to `replies`.
        thread.replies = replies;

        Ok(thread)
    }

    pub async fn find_reply(
        connection: &mut Connection<'_>,
        parent_id: &Id,
        reply_id: &Id,
    ) -> via::Result<ThreadDetails> {
        use crate::schema::threads::{id, thread_id};

        // Fetch the reply with the id = reply_id AND thread_id = parent_id.
        let reply = ThreadWithUser::query()
            .filter(id.eq(reply_id).and(thread_id.eq(parent_id)))
            .first_async(connection)
            .await?;

        // Side load the reactions to the reply.
        let reactions = Reaction::to_thread(connection, &reply.thread.id).await?;

        // Group the reactions with the reply.
        Ok(reply.into_details(reactions))
    }

    pub fn channel_id(&self) -> &Id {
        &self.thread.channel_id
    }

    pub(super) fn grouped_by(
        threads: Vec<ThreadWithUser>,
        reactions: Vec<ReactionPreview>,
    ) -> Vec<ThreadDetails> {
        let iter = reactions.grouped_by(&threads).into_iter();

        iter.zip(threads)
            .map(|(reactions, message)| message.into_details(reactions))
            .collect()
    }
}

impl ThreadWithUser {
    pub fn query() -> Select<JoinUsers, AsSelect<Self, Pg>> {
        threads::table
            .inner_join(users::table)
            .select(Self::as_select())
    }

    pub fn channel_id(&self) -> Id {
        self.thread.channel_id
    }

    pub(crate) fn into_delta(self) -> ThreadDelta {
        ThreadDelta {
            id: self.thread.id,
            body: self.thread.body,
            user: self.user,
            updated_at: self.thread.updated_at,
        }
    }

    fn into_details(self, reactions: Vec<ReactionPreview>) -> ThreadDetails {
        ThreadDetails {
            user: self.user,
            thread: self.thread,
            replies: Vec::new(),
            reactions,
        }
    }
}

impl HasTable for ThreadWithUser {
    type Table = threads::table;

    fn table() -> Self::Table {
        threads::table
    }
}

impl<'a> Identifiable for &'a ThreadWithUser {
    type Id = <&'a Thread as Identifiable>::Id;

    fn id(self) -> Self::Id {
        Identifiable::id(&self.thread)
    }
}

impl UpdatedThread {
    pub fn changed(self) -> Option<ThreadWithUser> {
        self.did_change.then_some(self.thread)
    }
}
