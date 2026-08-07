use diesel::associations::HasTable;
use diesel::helper_types::{AsSelect, InnerJoin, Select};
use diesel::pg::Pg;
use diesel::{prelude::*, sql_types};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use via::ResultExt;
use via_diesel::paginate::{Keyset, PER_PAGE};
use via_diesel::{AsyncQueryDsl, Paginate};

use super::{Channel, ReactionPreview, User, UserPreview};
use crate::app::Connection;
use crate::models::reaction::{Reaction, ReactionInThread, top_reactions_for};
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

#[derive(Serialize)]
pub struct ThreadDetails {
    #[serde(flatten)]
    thread: Thread,

    user: UserPreview,

    reactions: Vec<ReactionInThread>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    replies: Vec<ThreadDetails>,
}

via_diesel::filters! {
    pub fn by_id(id == Id) on threads;
    pub fn by_user(user_id == Id) on threads;
    pub fn by_thread(thread_id == Id) on threads;
    pub fn by_channel(channel_id == Id) on threads;

    pub fn is_thread(thread_id is_null) on threads;
    pub fn thread_id_is_null(thread_id is_null) on threads;
}

via_diesel::sorts! {
    pub fn recent(#[desc] created_at, #[desc] id) on threads;
}

impl Thread {
    pub async fn create(connection: &mut Connection<'_>, init: NewThread) -> via::Result<Self> {
        diesel::insert_into(threads::table)
            .values(init)
            .returning(Self::as_returning())
            .get_result_async(connection)
            .await
    }

    pub fn query() -> Select<JoinUsers, AsSelect<ThreadWithUser, Pg>> {
        threads::table
            .inner_join(users::table)
            .select(ThreadWithUser::as_select())
    }

    pub fn channel_id(&self) -> Id {
        self.channel_id
    }

    pub fn with_user(self, user: UserPreview) -> ThreadWithUser {
        ThreadWithUser { thread: self, user }
    }
}

impl ThreadWithUser {
    pub fn channel_id(&self) -> Id {
        self.thread.channel_id
    }

    pub fn with_reactions(self, reactions: Vec<ReactionInThread>) -> ThreadDetails {
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
