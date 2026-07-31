use diesel::backend::Backend;
use diesel::expression::AsExpression;
use diesel::serialize::{self, Output, ToSql};
use diesel::sql_types;
use std::str::FromStr;
use time::{OffsetDateTime, format_description::well_known};

/// An `OffsetDateTime` that can be parsed from Iso8601 date strings.
#[derive(AsExpression, Clone, Copy, Debug)]
#[diesel(sql_type = sql_types::Timestamptz)]
pub struct Iso8601(OffsetDateTime);

impl FromStr for Iso8601 {
    type Err = time::error::Parse;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        OffsetDateTime::parse(input, &well_known::Iso8601::DEFAULT).map(Self)
    }
}

impl<DB: Backend> ToSql<sql_types::Timestamptz, DB> for Iso8601
where
    OffsetDateTime: ToSql<sql_types::Timestamptz, DB>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, DB>) -> serialize::Result {
        self.0.to_sql(out)
    }
}
