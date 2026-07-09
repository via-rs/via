use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::pg::{Pg, PgValue};
use diesel::prelude::*;
use diesel::serialize::{self, Output, ToSql};
use diesel::sql_types::Text;
use diesel_async::RunQueryDsl;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt::{self, Debug, Formatter};
use time::OffsetDateTime;
use zeroize::Zeroizing;

use crate::database::{Connection, Id, users};
use crate::util::session::Unauthorized;

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

#[derive(Deserialize, Queryable, Selectable, Serialize)]
#[diesel(table_name = users)]
pub struct UserPreview {
    id: Id,
    username: String,
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
#[diesel(sql_type = Text)]
#[serde(transparent)]
struct Password {
    #[serde(deserialize_with = "deserialize_password")]
    hash: Zeroizing<String>,
}

filters! {
    pub fn by_id(id == &Id) on users;
    pub fn by_email(email == &str) on users;
}

sorts! {
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
    pub async fn authenticate(conn: &mut Connection<'_>, params: AuthParams) -> via::Result<Self> {
        use argon2::{Argon2, PasswordHash, PasswordVerifier};

        let AuthParams { email, password } = params;
        let password = password.as_bytes();

        let (user, hash) = users::table
            .select((Self::as_select(), users::password_hash))
            .filter(by_email(&email))
            .first::<(User, Password)>(conn)
            .await
            .or(Err(Unauthorized))?;

        PasswordHash::new(hash.value())
            .and_then(|hash| Argon2::default().verify_password(password, &hash))
            .map_or_else(|_| Err(Unauthorized.into()), |_| Ok(user))
    }

    pub async fn create(conn: &mut Connection<'_>, init: NewUser) -> via::Result<Self> {
        let user = diesel::insert_into(users::table)
            .values((
                users::email.eq(init.email),
                users::username.eq(init.username),
                users::password_hash.eq(init.password_hash),
            ))
            .returning(Self::as_returning())
            .get_result(conn)
            .await?;

        Ok(user)
    }
}

impl Password {
    fn value(&self) -> &str {
        &self.hash
    }
}

impl Debug for Password {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str("Password")
    }
}

impl FromSql<Text, Pg> for Password {
    fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
        let value = str::from_utf8(value.as_bytes())?;
        let Ok(hash) = argon2::PasswordHash::new(value) else {
            return Err("internal server error".into());
        };

        Ok(Password {
            hash: Zeroizing::new(hash.to_string()),
        })
    }
}

impl ToSql<Text, Pg> for Password {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        <str as ToSql<Text, Pg>>::to_sql(self.value(), out)
    }
}
