use diesel::helper_types::Offset;
use diesel::query_dsl::methods::{LimitDsl, OffsetDsl};
use via::request::params::QueryParams;

const MIN_PER_PAGE: i64 = 5;
const MAX_PER_PAGE: i64 = 100;

/// The default number of rows returned by a paginated query.
pub const PER_PAGE: i64 = 25;

pub trait Paginate<T> {
    type Output;
    fn page(self, cursor: T) -> Self::Output;
}

#[derive(Debug)]
pub struct LimitAndOffset {
    limit: i64,
    offset: i64,
}

#[derive(Debug)]
pub struct LimitAndPage {
    limit_and_offset: LimitAndOffset,
}

impl<T> Paginate<LimitAndOffset> for T
where
    T: LimitDsl,
    <T as LimitDsl>::Output: OffsetDsl,
{
    type Output = Offset<<T as LimitDsl>::Output>;

    fn page(self, cursor: LimitAndOffset) -> Self::Output {
        self.limit(cursor.limit).offset(cursor.offset)
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

impl TryFrom<QueryParams<'_>> for LimitAndOffset {
    type Error = via::Error;

    fn try_from(query: QueryParams<'_>) -> via::Result<Self> {
        Ok(Self {
            limit: query
                .first("limit")
                .ok_and_then(str::parse)?
                .unwrap_or(PER_PAGE)
                .clamp(MIN_PER_PAGE, MAX_PER_PAGE),

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
        let limit = query
            .first("limit")
            .ok_and_then(str::parse)?
            .unwrap_or(PER_PAGE)
            .clamp(MIN_PER_PAGE, MAX_PER_PAGE);

        if page < 1 {
            via::deny!(400, "page must be a positive integer");
        }

        Ok(Self {
            limit_and_offset: LimitAndOffset {
                offset: (page - 1).saturating_mul(limit),
                limit,
            },
        })
    }
}
