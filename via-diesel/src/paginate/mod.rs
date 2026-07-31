mod keyset;

pub use keyset::*;

use diesel::expression::AsExpression;
use diesel::helper_types::Offset;
use diesel::query_dsl::methods::{LimitDsl, OffsetDsl};
use diesel::sql_types;
use via::request::params::{QueryParam, QueryParams};

/// The default number of rows returned by a paginated query.
pub const PER_PAGE: i64 = 25;

pub trait Paginate<T> {
    type Output;
    fn page(self, cursor: T) -> Self::Output;
}

#[derive(AsExpression, Debug)]
#[diesel(sql_type = sql_types::BigInt)]
pub struct Limit(i64);

#[derive(Debug)]
pub struct LimitAndPage {
    limit_and_offset: LimitAndOffset,
}

#[derive(Debug)]
pub struct LimitAndOffset {
    limit: Limit,
    offset: i64,
}

impl Limit {
    pub const PER_PAGE: Self = Self(PER_PAGE);

    #[inline]
    pub fn value(&self) -> i64 {
        self.0
    }
}

impl Limit {
    #[inline]
    fn new(value: i64) -> Self {
        const MIN_PER_PAGE: i64 = 5;
        const MAX_PER_PAGE: i64 = 100;

        Self(value.clamp(MIN_PER_PAGE, MAX_PER_PAGE))
    }
}

impl<T> Paginate<LimitAndOffset> for T
where
    T: LimitDsl,
    <T as LimitDsl>::Output: OffsetDsl,
{
    type Output = Offset<<T as LimitDsl>::Output>;

    fn page(self, cursor: LimitAndOffset) -> Self::Output {
        self.limit(cursor.limit.0).offset(cursor.offset)
    }
}

impl<T> Paginate<LimitAndPage> for T
where
    T: LimitDsl,
    <T as LimitDsl>::Output: OffsetDsl,
{
    type Output = Offset<<T as LimitDsl>::Output>;

    fn page(self, cursor: LimitAndPage) -> Self::Output {
        self.page(cursor.limit_and_offset)
    }
}

impl TryFrom<QueryParam<'_, '_>> for Limit {
    type Error = via::Error;

    fn try_from(param: QueryParam<'_, '_>) -> Result<Self, Self::Error> {
        param
            .ok_and_then(str::parse)?
            .map_or(Ok(Self::PER_PAGE), |value| Ok(Self::new(value)))
    }
}

impl TryFrom<QueryParams<'_>> for LimitAndOffset {
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

impl TryFrom<QueryParams<'_>> for LimitAndPage {
    type Error = via::Error;

    fn try_from(query: QueryParams<'_>) -> via::Result<Self> {
        let page = query.first("page").ok_and_then(str::parse)?.unwrap_or(1i64);
        let limit = Limit::try_from(query.first("limit"))?;

        if page < 1 {
            via::deny!(400, "page must be a positive integer");
        }

        Ok(Self {
            limit_and_offset: LimitAndOffset {
                offset: (page - 1).saturating_mul(limit.0),
                limit,
            },
        })
    }
}
