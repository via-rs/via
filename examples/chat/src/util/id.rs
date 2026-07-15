use diesel::backend::Backend;
use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::pg::Pg;
use diesel::serialize::{self, Output, ToSql};
use diesel::sql_types::BigInt;
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;
use via_pubsub::Interest;

#[derive(
    AsExpression, Clone, Copy, Debug, Deserialize, Eq, FromSqlRow, Hash, PartialEq, Serialize,
)]
#[diesel(sql_type = BigInt)]
pub struct Id(i64);

#[derive(Debug)]
pub struct InvalidIdError;

impl Id {
    pub fn new(value: i64) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i64 {
        self.0
    }
}

// A marker trait that means Copy + Eq + Hash.
impl Interest for Id {}

impl Display for Id {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl FromStr for Id {
    type Err = InvalidIdError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if let Ok(uuid) = input.parse() {
            Ok(Self(uuid))
        } else {
            Err(InvalidIdError)
        }
    }
}

impl FromSql<BigInt, Pg> for Id {
    fn from_sql(bytes: <Pg as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        FromSql::from_sql(bytes).map(Self)
    }
}

impl<DB: Backend> ToSql<BigInt, DB> for Id
where
    i64: ToSql<BigInt, DB>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, DB>) -> serialize::Result {
        self.0.to_sql(out)
    }
}

impl std::error::Error for InvalidIdError {}

impl Display for InvalidIdError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "invalid uuid")
    }
}
