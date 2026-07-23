use diesel::backend::Backend;
use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::pg::Pg;
use diesel::serialize::{self, Output, ToSql};
use diesel::{AsExpression, sql_types};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;
use uuid::Uuid;

#[derive(
    AsExpression, Clone, Copy, Debug, Deserialize, Eq, FromSqlRow, Hash, PartialEq, Serialize,
)]
#[diesel(sql_type = sql_types::Uuid)]
pub struct Id(Uuid);

#[derive(Debug)]
pub struct InvalidIdError;

impl Id {
    pub fn new(value: Uuid) -> Self {
        Self(value)
    }

    pub fn value(&self) -> Uuid {
        self.0
    }
}

impl Display for Id {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl FromStr for Id {
    type Err = InvalidIdError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        input
            .parse()
            .map_or(Err(InvalidIdError), |uuid| Ok(Self(uuid)))
    }
}

impl FromSql<sql_types::Uuid, Pg> for Id {
    fn from_sql(bytes: <Pg as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        FromSql::from_sql(bytes).map(Self)
    }
}

impl<DB: Backend> ToSql<sql_types::Uuid, DB> for Id
where
    Uuid: ToSql<sql_types::Uuid, DB>,
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
