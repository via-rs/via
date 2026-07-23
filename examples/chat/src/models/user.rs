use argon2::PasswordHash;
use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::helper_types::{AsSelect, InnerJoin, InnerJoinOn, Select};
use diesel::pg::{Pg, PgValue};
use diesel::prelude::*;
use diesel::serialize::{self, Output, ToSql};
use diesel::{AsExpression, dsl::count, sql_types};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt::{self, Debug, Formatter};
use time::OffsetDateTime;
use via_diesel::AsyncQueryDsl;
use zeroize::Zeroizing;

use crate::app::Connection;
use crate::models::subscription::{self, ChannelSubscription};
use crate::schema::{channels, subscriptions, users};
use crate::util::{Id, session::Unauthorized};

type JoinSubscriptions = InnerJoin<users::table, subscriptions::table>;

type JoinChannels = InnerJoinOn<JoinSubscriptions, channels::table, ThroughSubscriptions>;
type ThroughSubscriptions = diesel::dsl::Eq<subscriptions::channel_id, channels::id>;

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

#[derive(Deserialize)]
pub struct NewUser {
    email: String,
    username: String,
    #[serde(rename = "password")]
    password_hash: Password,
}

#[derive(AsChangeset, Deserialize)]
#[diesel(table_name = users)]
pub struct ChangeSet {
    email: Option<String>,
    username: Option<String>,
}

#[derive(Debug, Deserialize, Queryable, Selectable, Serialize)]
#[diesel(table_name = users)]
pub struct UserPreview {
    id: Id,
    email: String,
    username: String,
}

pub struct UserWithSubscriptions {
    id: Id,
    email: String,
    username: String,
    subscriptions: Vec<ChannelSubscription>,
}

#[derive(Deserialize)]
pub struct AuthParams {
    email: String,
    password: Zeroizing<String>,
}

/// A write-only password credential.
///
/// This type should never expose plaintext password material.
///
/// When deserialized from request input, the plaintext password is immediately
/// hashed with Argon2id.
///
/// When loaded from the database, the stored PHC string is validated and
/// retained only for verification purposes.
#[derive(AsExpression, Deserialize, FromSqlRow)]
#[diesel(sql_type = sql_types::Text)]
#[serde(transparent)]
struct Password {
    #[serde(deserialize_with = "deserialize_password")]
    hash: Zeroizing<String>,
}

via_diesel::filters! {
    pub fn by_id(id == Id) on users;
    pub fn by_email(email == &str) on users;
}

via_diesel::sorts! {
    pub fn recent(#[desc] created_at, id) on users;
}

/// Deserializes a plaintext password directly into an opaque password hash.
fn deserialize_password<'de, D>(deserializer: D) -> Result<Zeroizing<String>, D::Error>
where
    D: Deserializer<'de>,
{
    use argon2::password_hash::{SaltString, rand_core::OsRng};
    use argon2::{Argon2, PasswordHasher};
    use serde::de::Error;

    // Deserialize the password as plaintext wrapped in Zeroizing.
    //
    // This prevents the plaintext password from lingering in memory for
    // longer than it has to.
    let password = Zeroizing::new(String::deserialize(deserializer)?);

    // Shadow the plain text password so it can only be used as a byte slice.
    let password = password.as_bytes();

    Argon2::default()
        // Hash the plain text password using the Argon2id algorithm.
        .hash_password(password, &SaltString::generate(&mut OsRng))
        // Extract a Zeroizing<String> of the hash from the result.
        .map_or_else(
            // If an error occurs, return an opaque error message.
            |_| Err(Error::custom("internal server error")),
            // Otherwise, map the password hash to a Zeroizing<String>.
            |hash| Ok(Zeroizing::new(hash.to_string())),
        )
}

impl User {
    pub async fn authenticate(
        connection: &mut Connection<'_>,
        params: AuthParams,
    ) -> via::Result<Self> {
        use argon2::{Argon2, PasswordHash, PasswordVerifier};

        let (user, password) = users::table
            .select((Self::as_select(), users::password_hash))
            .filter(by_email(&params.email))
            .first_async::<(User, Password)>(connection)
            .await
            .or(Err(Unauthorized))?;

        Argon2::default()
            .verify_password(params.password.as_bytes(), &password.hash()?)
            .map_or_else(|_| Err(Unauthorized.into()), |_| Ok(user))
    }

    pub async fn create(connection: &mut Connection<'_>, init: NewUser) -> via::Result<Self> {
        let values = (
            users::email.eq(init.email),
            users::username.eq(init.username),
            users::password_hash.eq(init.password_hash),
        );

        diesel::insert_into(users::table)
            .values(values)
            .returning(Self::as_returning())
            .get_result_async(connection)
            .await
    }

    pub async fn destroy(connection: &mut Connection<'_>, id: Id) -> via::Result<usize> {
        diesel::delete(users::table)
            .filter(by_id(id))
            .execute_async(connection)
            .await
    }

    pub async fn update(
        connection: &mut Connection<'_>,
        id: Id,
        changes: ChangeSet,
    ) -> via::Result<Self> {
        diesel::update(users::table)
            .filter(by_id(id))
            .set(changes)
            .returning(Self::as_returning())
            .get_result_async(connection)
            .await
    }

    pub fn query() -> Select<users::table, AsSelect<Self, Pg>> {
        users::table.select(Self::as_select())
    }

    pub async fn exists(connection: &mut Connection<'_>, id: Id) -> via::Result<()> {
        let total = users::table
            .select(count(users::id))
            .filter(by_id(id))
            .get_result_async::<i64>(connection)
            .await?;

        if total == 1 {
            Ok(())
        } else {
            Err(Unauthorized.into())
        }
    }

    pub async fn with_subscriptions(
        connection: &mut Connection<'_>,
        id: Id,
    ) -> via::Result<UserWithSubscriptions> {
        let user = UserPreview::query()
            .filter(by_id(id))
            .first_async(connection)
            .await?;

        let subscriptions = ChannelSubscription::query()
            .filter(subscription::by_user(id).and(subscription::can_participate()))
            .limit(100)
            .load_async(connection)
            .await?;

        Ok(UserWithSubscriptions {
            id: user.id,
            email: user.email,
            username: user.username,
            subscriptions,
        })
    }
}

impl UserPreview {
    pub fn query() -> Select<users::table, AsSelect<Self, Pg>> {
        users::table.select(Self::as_select())
    }

    pub fn id(&self) -> Id {
        self.id
    }
}

impl UserWithSubscriptions {
    pub fn id(&self) -> Id {
        self.id
    }

    pub fn subscriptions(&self) -> impl Iterator<Item = &ChannelSubscription> {
        self.subscriptions.iter()
    }

    pub fn to_preview(&self) -> UserPreview {
        UserPreview {
            id: self.id,
            email: self.email.clone(),
            username: self.username.clone(),
        }
    }
}

impl Password {
    #[inline]
    fn as_bytes(&self) -> &[u8] {
        self.hash.as_bytes()
    }

    #[inline]
    fn hash(&self) -> via::Result<PasswordHash<'_>> {
        match PasswordHash::new(self.hash.as_str()) {
            Ok(hash) => Ok(hash),
            Err(_) => Err(via::err!(500, "internal server error")),
        }
    }
}

impl Debug for Password {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str("Password")
    }
}

impl FromSql<sql_types::Text, Pg> for Password {
    fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
        let value = str::from_utf8(value.as_bytes())?;
        let Ok(hash) = PasswordHash::new(value) else {
            return Err("internal server error".into());
        };

        Ok(Password {
            hash: Zeroizing::new(hash.to_string()),
        })
    }
}

impl ToSql<sql_types::Text, Pg> for Password {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        <str as ToSql<sql_types::Text, Pg>>::to_sql(&self.hash, out)
    }
}
