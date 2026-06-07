use serde::de::{Deserializer, Error as DeError};
use serde::{Deserialize, Serialize, Serializer};
use std::fmt::{self, Debug, Display, Formatter};
use time::OffsetDateTime;
use uuid::Uuid;
use via::{Error, deny};

use crate::database::Id;

const MAX_EMOJI_LEN: usize = 16;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reaction {
    id: Uuid,
    #[serde(
        deserialize_with = "deserialize_emoji",
        serialize_with = "serialize_emoji"
    )]
    emoji: Emoji,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,

    conversation_id: Uuid,
    user_id: Id,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewReaction {
    pub conversation_id: Option<Uuid>,
    pub user_id: Option<Id>,

    #[serde(deserialize_with = "deserialize_emoji")]
    emoji: Emoji,
}

#[derive(Clone, Debug)]
struct Emoji {
    buf: [u8; MAX_EMOJI_LEN],
    len: usize,
}

fn deserialize_emoji<'de, D>(deserializer: D) -> Result<Emoji, D::Error>
where
    D: Deserializer<'de>,
{
    let input = <&str as Deserialize>::deserialize(deserializer)?;

    if input.len() > MAX_EMOJI_LEN {
        Err(D::Error::custom(
            "emoji would overflow maximum byte len: 16.",
        ))
    } else {
        let mut buf = [0; MAX_EMOJI_LEN];
        let len = input.len();

        buf[..len].copy_from_slice(input.as_bytes());
        Ok(Emoji { buf, len })
    }
}

fn serialize_emoji<S>(emoji: &Emoji, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(emoji.as_str())
}

impl Emoji {
    #[inline(always)]
    fn as_str(&self) -> &str {
        // Safety: The bytes in self are guaranteed to be UTF-8.
        unsafe { str::from_utf8_unchecked(&self.buf[..self.len]) }
    }
}

impl Display for Emoji {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Reaction {
    pub fn new(new_reaction: NewReaction) -> Result<Self, Error> {
        let Some(conversation_id) = new_reaction.conversation_id else {
            deny!(500, "conversation_id is required.");
        };

        let Some(user_id) = new_reaction.user_id else {
            deny!(500, "user_id is required.");
        };

        Ok(Self {
            id: Uuid::new_v4(),
            emoji: new_reaction.emoji,
            created_at: OffsetDateTime::now_utc(),
            updated_at: OffsetDateTime::now_utc(),
            conversation_id,
            user_id,
        })
    }
}
