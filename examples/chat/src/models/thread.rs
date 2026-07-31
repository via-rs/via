use diesel::associations::HasTable;
use diesel::helper_types::{AsSelect, InnerJoin, Select};
use diesel::pg::Pg;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use via::ResultExt;
use via_diesel::paginate::{Keyset, PER_PAGE};
use via_diesel::{AsyncQueryDsl, Paginate};

use super::{Channel, ReactionPreview, User, UserPreview};
use crate::app::Connection;
use crate::models::Reaction;
use crate::schema::{threads, users};
use crate::util::Id;

pub type JoinUsers = InnerJoin<threads::table, users::table>;
pub type SelectThreadWithUser = Select<JoinUsers, AsSelect<ThreadWithUser, Pg>>;

#[derive(Associations, Debug, Deserialize, Identifiable, Queryable, Selectable, Serialize)]
#[diesel(belongs_to(Channel))]
#[diesel(belongs_to(Thread, foreign_key = thread_id))]
#[diesel(belongs_to(User))]
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

#[derive(Debug, Deserialize, Insertable)]
#[diesel(table_name = threads)]
#[serde(rename_all = "camelCase")]
pub struct NewThread {
    pub channel_id: Option<Id>,
    pub thread_id: Option<Id>,
    pub user_id: Option<Id>,
    body: String,
}

#[derive(AsChangeset, Deserialize)]
#[diesel(table_name = threads)]
pub struct ChangeSet {
    body: String,
}

#[derive(Debug, Deserialize, Queryable, Selectable, Serialize)]
#[diesel(table_name = threads)]
#[diesel(check_for_backend(Pg))]
pub struct ThreadWithUser {
    #[diesel(embed)]
    #[serde(flatten)]
    thread: Thread,

    #[diesel(embed)]
    user: UserPreview,
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

via_diesel::filters! {
    pub fn by_id(id == Id) on threads;
    pub fn by_user(user_id == Id) on threads;
    pub fn by_thread(thread_id == Id) on threads;
    pub fn by_channel(channel_id == Id) on threads;

    pub fn is_thread(thread_id is_null) on threads;
}

via_diesel::sorts! {
    pub fn recent(#[desc] created_at, id) on threads;
}

impl Thread {
    pub async fn find(connection: &mut Connection<'_>, id: Id) -> via::Result<ThreadDetails> {
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
            let threads = threads::table
                .inner_join(target.on(target_id.eq(id).and(belongs_to_target_root)))
                .inner_join(users::table.on(users::id.eq(threads::user_id)))
                .select(ThreadWithUser::as_select())
                .filter(by_id(id).or(by_thread(id)))
                .order(recent())
                .limit(PER_PAGE + 1)
                .load_async(connection)
                .await?;

            // Side load the reactions to the threads in `threads`.
            Reaction::to_threads(connection, threads).await?
        };

        // Pop the parent thread from `replies`.
        // They are ordered by recent. Therefore, the parent is always last.
        let mut thread = replies.pop().or_not_found()?;

        // Set the replies field of `thread` to `replies`.
        thread.replies = replies;

        // Reverse the replies to match their render sequence.
        thread.replies.reverse();

        Ok(thread)
    }

    pub async fn create(connection: &mut Connection<'_>, init: NewThread) -> via::Result<Self> {
        diesel::insert_into(threads::table)
            .values(init)
            .returning(Self::as_returning())
            .get_result_async(connection)
            .await
    }

    pub fn query() -> threads::table {
        threads::table
    }

    pub fn with_user(self, user: UserPreview) -> ThreadWithUser {
        ThreadWithUser { thread: self, user }
    }

    pub fn channel_id(&self) -> Id {
        self.channel_id
    }
}

impl ThreadDetails {
    pub fn grouped_by(
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

    pub fn into_details(self, reactions: Vec<ReactionPreview>) -> ThreadDetails {
        ThreadDetails {
            user: self.user,
            thread: self.thread,
            replies: Vec::new(),
            reactions,
        }
    }

    pub fn into_thread(
        self,
        reactions: Vec<ReactionPreview>,
        replies: Vec<ThreadDetails>,
    ) -> ThreadDetails {
        ThreadDetails {
            thread: self.thread,
            reactions,
            replies,
            user: self.user,
        }
    }

    pub fn channel_id(&self) -> Id {
        self.thread.channel_id
    }
}

impl HasTable for ThreadWithUser {
    type Table = threads::table;

    fn table() -> Self::Table {
        threads::table
    }
}

impl<'a> Identifiable for &'_ &'a ThreadWithUser {
    type Id = <&'a Thread as Identifiable>::Id;

    fn id(self) -> Self::Id {
        Identifiable::id(*self)
    }
}

impl<'a> Identifiable for &'a ThreadWithUser {
    type Id = <&'a Thread as Identifiable>::Id;

    fn id(self) -> Self::Id {
        Identifiable::id(&self.thread)
    }
}
