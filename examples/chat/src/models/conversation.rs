use chrono::{DateTime, Utc};
use diesel::associations::HasTable;
use diesel::helper_types::InnerJoin;
use diesel::pg::Pg;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use super::channel::Channel;
use super::reaction::ReactionPreview;
use super::user::{User, UserPreview};
use crate::schema::{conversations, users};
use crate::util::Id;

#[derive(Associations, Identifiable, Queryable, Selectable, Serialize)]
#[diesel(belongs_to(Conversation, foreign_key = thread_id))]
#[diesel(belongs_to(Channel))]
#[diesel(belongs_to(User))]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    id: Id,
    body: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    total_reactions: i64,
    total_replies: i64,

    #[serde(skip)]
    channel_id: Id,

    #[serde(skip)]
    thread_id: Option<Id>,

    #[serde(skip)]
    user_id: Id,
}

#[derive(Deserialize, Insertable)]
#[diesel(table_name = conversations)]
#[serde(rename_all = "camelCase")]
pub struct NewConversation {
    pub channel_id: Option<Id>,
    pub thread_id: Option<Id>,
    pub user_id: Option<Id>,
    body: String,
}

#[derive(AsChangeset, Deserialize)]
#[diesel(table_name = conversations)]
pub struct ChangeSet {
    body: String,
}

#[derive(Serialize)]
pub struct ConversationDetails {
    #[serde(flatten)]
    conversation: Conversation,

    reactions: Vec<ReactionPreview>,

    user: UserPreview,
}

#[derive(Queryable, Selectable, Serialize)]
#[diesel(table_name = conversations)]
#[diesel(check_for_backend(Pg))]
pub struct ConversationWithUser {
    #[diesel(embed)]
    #[serde(flatten)]
    conversation: Conversation,

    #[diesel(embed)]
    user: UserPreview,
}

#[derive(Serialize)]
pub struct ThreadDetails {
    #[serde(flatten)]
    conversation: Conversation,

    reactions: Vec<ReactionPreview>,

    #[serde(skip_serializing_if = "Option::is_none")]
    replies: Option<Vec<ConversationDetails>>,

    user: UserPreview,
}

filters! {
    pub fn by_id(id == &Id) on conversations;
    pub fn by_user(user_id == &Id) on conversations;
    pub fn by_thread(thread_id == &Id) on conversations;
    pub fn by_channel(channel_id == &Id) on conversations;

    pub fn is_thread(thread_id is_null) on conversations;
}

sorts! {
    pub fn recent(#[desc] created_at, id) on conversations;
}

impl Conversation {
    pub fn query() -> conversations::table {
        conversations::table
    }

    pub fn with_author() -> InnerJoin<conversations::table, users::table> {
        Self::query().inner_join(users::table)
    }

    pub fn channel_id(&self) -> &Id {
        &self.channel_id
    }
}

impl ConversationDetails {
    pub fn grouped_by(
        conversations: Vec<ConversationWithUser>,
        reactions: Vec<ReactionPreview>,
    ) -> Vec<ConversationDetails> {
        let iter = reactions.grouped_by(&conversations).into_iter();

        iter.zip(conversations)
            .map(|(reactions, message)| message.into_details(reactions))
            .collect()
    }
}

impl ConversationWithUser {
    pub fn into_details(self, reactions: Vec<ReactionPreview>) -> ConversationDetails {
        ConversationDetails {
            conversation: self.conversation,
            reactions,
            user: self.user,
        }
    }

    pub fn into_thread(
        self,
        reactions: Vec<ReactionPreview>,
        replies: Option<Vec<ConversationDetails>>,
    ) -> ThreadDetails {
        ThreadDetails {
            conversation: self.conversation,
            reactions,
            replies,
            user: self.user,
        }
    }
}

impl HasTable for ConversationWithUser {
    type Table = conversations::table;

    fn table() -> Self::Table {
        conversations::table
    }
}

impl<'a> Identifiable for &'_ &'a ConversationWithUser {
    type Id = <&'a Conversation as Identifiable>::Id;

    fn id(self) -> Self::Id {
        Identifiable::id(*self)
    }
}

impl<'a> Identifiable for &'a ConversationWithUser {
    type Id = <&'a Conversation as Identifiable>::Id;

    fn id(self) -> Self::Id {
        Identifiable::id(&self.conversation)
    }
}
