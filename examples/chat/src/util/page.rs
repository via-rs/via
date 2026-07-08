use diesel::Expression;
use diesel::dsl::Offset;
use diesel::query_dsl::methods::{LimitDsl, OffsetDsl};
use diesel::sql_types::{BigInt, Timestamptz};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use via::request::QueryParams;
use via::{Error, ResultExt, deny};

use crate::database::Id;

const INVALID_DATE_TIME: &str = "invalid datetime for keyset in query \"before\".";
const INVALID_KEYSET: &str = "invalid keyset in query \"before\".";
const INVALID_ID: &str = "invalid id for keyset in query \"before\".";

const MIN_PER_PAGE: i64 = 25;
const MAX_PER_PAGE: i64 = 50;

pub const PER_PAGE: i64 = 25;

// type KeysetAfter<'a, A, B> = after_keyset<&'a OffsetDateTime, &'a Id, A, B>;
type KeysetBefore<A, B> = after_keyset<A, B, OffsetDateTime, Id>;

pub trait Paginate<T> {
    type Output;
    fn paginate(self, page: T) -> Self::Output;
}

#[derive(Debug)]
pub struct Keyset {
    pub limit: i64,
    pub value: (OffsetDateTime, Id),
}

#[derive(Debug)]
pub struct Page {
    limit: i64,
    offset: i64,
}

diesel::define_sql_function! {
    /// SQL: (lhs0, lhs1) > (rhs0, rhs1)
    fn after_keyset(lhs0: Timestamptz, lhs1: BigInt, rhs0: Timestamptz, rhs1: BigInt) -> Bool;
}

fn limit_from_query(query: &QueryParams) -> via::Result<i64> {
    let limit = query
        .first("limit")
        .ok()?
        .map_or(Ok(MIN_PER_PAGE), |value| value.parse())?
        .clamp(MIN_PER_PAGE, MAX_PER_PAGE);

    Ok(limit)
}

fn parse_iso_8601(input: &str) -> via::Result<OffsetDateTime> {
    OffsetDateTime::parse(input, &Rfc3339).or_bad_request()
}

impl Keyset {
    // pub fn after<A, B>(&self, pivot: (A, B)) -> KeysetAfter<A, B>
    // where
    //     A: Expression<SqlType = Timestamptz>,
    //     B: Expression<SqlType = Uuid>,
    // {
    //     keyset_expr(self.value.0, self.value.1, pivot.0, pivot.1)
    // }

    pub fn before<A, B>(&self, pivot: (A, B)) -> KeysetBefore<A, B>
    where
        A: Expression<SqlType = Timestamptz>,
        B: Expression<SqlType = BigInt>,
    {
        let (timestamp, pk) = self.value;
        after_keyset(pivot.0, pivot.1, timestamp, pk)
    }
}

impl TryFrom<QueryParams<'_>> for Keyset {
    type Error = Error;

    fn try_from(query: QueryParams<'_>) -> Result<Self, Self::Error> {
        let value = query.first("after").percent_decode().or_bad_request()?;
        let Some((created_at, id)) = value.split_once(',') else {
            deny!(400, "{}", INVALID_KEYSET);
        };

        Ok(Self {
            limit: limit_from_query(&query)?,
            value: (parse_iso_8601(created_at)?, id.parse()?),
        })
    }
}

impl<T> Paginate<Page> for T
where
    T: LimitDsl,
    <T as LimitDsl>::Output: OffsetDsl,
{
    type Output = Offset<<T as LimitDsl>::Output>;

    fn paginate(self, page: Page) -> Self::Output {
        self.limit(page.limit).offset(page.offset)
    }
}

impl TryFrom<QueryParams<'_>> for Page {
    type Error = Error;

    fn try_from(query: QueryParams<'_>) -> Result<Self, Self::Error> {
        let limit = limit_from_query(&query)?;
        let page = query
            .first("page")
            .ok()?
            .map_or(Ok(1i64), |value| value.parse())
            .or_bad_request()?;

        if page < 1 {
            deny!(400, "page must be a positive integer");
        }

        Ok(Self {
            limit,
            offset: (page - 1).saturating_mul(limit),
        })
    }
}
