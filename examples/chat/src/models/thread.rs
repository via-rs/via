use diesel::associations::HasTable;
use diesel::dsl::{AsSelect, Select};
use diesel::helper_types::InnerJoin;
use diesel::pg::Pg;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::{Channel, ReactionPreview, User, UserPreview};
use crate::schema::{threads, users};
use crate::util::Id;

type JoinUsers = InnerJoin<threads::table, users::table>;

#[derive(Associations, Deserialize, Identifiable, Queryable, Selectable, Serialize)]
#[diesel(belongs_to(Channel))]
#[diesel(belongs_to(Thread, foreign_key = thread_id))]
#[diesel(belongs_to(User))]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    id: Id,
    body: String,

    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,

    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,

    total_reactions: i64,
    total_replies: i64,

    channel_id: Id,
    thread_id: Option<Id>,
    user_id: Id,
}

#[derive(Deserialize, Insertable)]
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

#[derive(Queryable, Selectable, Serialize)]
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

    reactions: Vec<ReactionPreview>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    replies: Vec<ThreadDetails>,

    user: UserPreview,
}

via_diesel::filters! {
    pub fn by_id(id == &Id) on threads;
    pub fn by_user(user_id == &Id) on threads;
    pub fn by_thread(thread_id == &Id) on threads;
    pub fn by_channel(channel_id == &Id) on threads;

    pub fn is_thread(thread_id is_null) on threads;
}

via_diesel::sorts! {
    pub fn recent(#[desc] created_at, id) on threads;
}

// fn split_own_reactions(reactions: &mut Vec<ReactionPreview>, id: &Id) -> Vec<ReactionPreview> {
//     // Sort the reactions vec so that our own reactions are last.
//     reactions.sort_by_key(|reaction| *id == reaction.to_id());

//     // Find the first index of a reaction that belongs to the id predicate.
//     let pivot = reactions
//         .iter()
//         .position(|reaction| *id == reaction.to_id())
//         .unwrap_or(reactions.len());

//     reactions.split_off(pivot)
// }

impl Thread {
    // pub async fn find(connection: &mut Connection<'_>, id: &Id) -> via::Result<ThreadDetails> {
    //     // Load the thread by id along with the first page of replies.
    //     let mut threads = Thread::with_author()
    //         .select(ThreadWithUser::as_select())
    //         .filter(by_id(id).or(by_thread(id)))
    //         .order(recent())
    //         .limit(PER_PAGE + 1)
    //         .load(connection)
    //         .await?;

    //     // Aggregate the reactions for each thread.
    //     let mut reactions = {
    //         let ids = threads.iter().map(Identifiable::id);
    //         Reaction::to_threads(connection, ids).await?
    //     };

    //     // Take the last thread from the vec of threads. The query
    //     // orders threads by created_at DESC. Therefore, the last item in the
    //     // vec is always the parent.
    //     let thread = threads.pop().or_not_found()?;

    //     // Move the reactions to thread into their own vec.
    //     let own_reactions = split_own_reactions(&mut reactions, id);

    //     // Group replies to the thread with with their reactions.
    //     let replies = ThreadDetails::grouped_by(threads, reactions);

    //     Ok(thread.into_thread(own_reactions, replies))
    // }

    pub fn query() -> threads::table {
        threads::table
    }

    pub fn with_author() -> InnerJoin<threads::table, users::table> {
        Self::query().inner_join(users::table)
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
