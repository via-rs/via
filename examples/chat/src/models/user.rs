use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::database::Id;
use crate::database::users;

#[derive(Clone, Deserialize, Identifiable, Queryable, Selectable, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    id: Id,
    email: String,
    username: String,

    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,

    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
}

#[derive(Deserialize, Insertable)]
#[diesel(table_name = users)]
pub struct NewUser {
    email: String,
    username: String,
}

#[derive(AsChangeset, Deserialize)]
#[diesel(table_name = users)]
pub struct ChangeSet {
    email: Option<String>,
    username: Option<String>,
}

#[derive(Deserialize, Queryable, Selectable, Serialize)]
#[diesel(table_name = users)]
pub struct UserPreview {
    id: Id,
    username: String,
}

filters! {
    pub fn by_id(id == &Id) on users;
    pub fn by_username(username == &str) on users;
}

sorts! {
    pub fn recent(#[desc] created_at, id) on users;
}
