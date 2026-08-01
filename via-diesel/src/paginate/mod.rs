mod keyset;
mod limit;

pub use keyset::*;

use diesel::helper_types::Offset;
use diesel::query_dsl::methods::{LimitDsl, OffsetDsl};
use via::{deny, request::QueryParams};

use limit::{DEFAULT_MAX_LIMIT, Limit};

/// The default number of rows returned by a paginated query.
pub const PER_PAGE: i64 = 25;

pub trait Paginate<T> {
    type Output;
    fn page(self, cursor: T) -> Self::Output;
}

#[derive(Debug)]
pub struct LimitAndPage<const MAX: i64 = DEFAULT_MAX_LIMIT> {
    limit: Limit<MAX>,
    page: i64,
}

#[derive(Debug)]
pub struct LimitAndOffset<const MAX: i64 = DEFAULT_MAX_LIMIT> {
    limit: Limit<MAX>,
    offset: i64,
}

impl<T, const MAX: i64> Paginate<LimitAndOffset<MAX>> for T
where
    T: LimitDsl,
    <T as LimitDsl>::Output: OffsetDsl,
{
    type Output = Offset<<T as LimitDsl>::Output>;

    fn page(self, cursor: LimitAndOffset<MAX>) -> Self::Output {
        self.limit(cursor.limit.value()).offset(cursor.offset)
    }
}

impl<T, const MAX: i64> Paginate<LimitAndPage<MAX>> for T
where
    T: LimitDsl,
    <T as LimitDsl>::Output: OffsetDsl,
{
    type Output = Offset<<T as LimitDsl>::Output>;

    fn page(self, cursor: LimitAndPage<MAX>) -> Self::Output {
        let limit = cursor.limit.value();
        let offset = limit.saturating_mul(cursor.page.max(1) - 1);

        self.limit(limit).offset(offset)
    }
}

impl<const MAX: i64> TryFrom<QueryParams<'_>> for LimitAndOffset<MAX> {
    type Error = via::Error;

    fn try_from(query: QueryParams<'_>) -> via::Result<Self> {
        Ok(Self {
            limit: query.first("limit").try_into()?,
            offset: query
                .first("offset")
                .ok_and_then::<_, i64, _>(str::parse)?
                .unwrap_or_default()
                .max(0),
        })
    }
}

impl<const MAX: i64> TryFrom<QueryParams<'_>> for LimitAndPage<MAX> {
    type Error = via::Error;

    fn try_from(query: QueryParams<'_>) -> via::Result<Self> {
        let limit = Limit::try_from(query.first("limit"))?;
        let page = query.first("page").ok_and_then(str::parse)?.unwrap_or(1i64);

        if page < 1 {
            deny!(400, "page must be a positive integer");
        }

        Ok(Self { limit, page })
    }
}
