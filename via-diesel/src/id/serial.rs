use diesel::backend::Backend;
use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::pg::Pg;
use diesel::serialize::{self, Output, ToSql};
use diesel::{AsExpression, sql_types};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};
use std::num::ParseIntError;
use std::str::FromStr;

pub type SqlType = sql_types::BigInt;

#[derive(
    AsExpression,
    Clone,
    Copy,
    Debug,
    Deserialize,
    Eq,
    FromSqlRow,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
)]
#[diesel(sql_type = sql_types::BigInt)]
pub struct Id(i64);

#[derive(Debug)]
pub struct InvalidIdError(ParseIntError);

impl Id {
    pub const fn new(value: i64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> i64 {
        self.0
    }
}

impl Display for Id {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl FromStr for Id {
    type Err = InvalidIdError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        input.parse().map(Self).map_err(InvalidIdError)
    }
}

impl FromSql<sql_types::BigInt, Pg> for Id {
    fn from_sql(bytes: <Pg as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        i64::from_sql(bytes).map(Self)
    }
}

impl<DB> ToSql<sql_types::BigInt, DB> for Id
where
    DB: Backend,
    i64: ToSql<sql_types::BigInt, DB>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, DB>) -> serialize::Result {
        self.0.to_sql(out)
    }
}

impl std::error::Error for InvalidIdError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl Display for InvalidIdError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "invalid id")
    }
}
