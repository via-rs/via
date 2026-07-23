use diesel::dsl::Offset;
use diesel::query_dsl::methods::{LimitDsl, OffsetDsl};
use via::request::params::{QueryParam, QueryParams};
use via::{Error, ResultExt};

const MIN_PER_PAGE: i64 = 5;
const MAX_PER_PAGE: i64 = 100;

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

fn map_or<F, T, E>(param: QueryParam, default: T, op: F) -> Result<T, Error>
where
    F: FnOnce(&str) -> Result<T, E>,
    Error: From<E>,
{
    param.ok().and_then(|option| match option.as_deref() {
        Some(value) => op(value).or_bad_request(),
        None => Ok(default),
    })
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
        let limit = map_or(query.first("limit"), MIN_PER_PAGE, str::parse)?;
        let offset = map_or(query.first("offset"), 0, str::parse)?;

        Ok(Self {
            limit: limit.clamp(MIN_PER_PAGE, MAX_PER_PAGE),
            offset: offset.max(0),
        })
    }
}

impl TryFrom<QueryParams<'_>> for LimitAndPage {
    type Error = via::Error;

    fn try_from(query: QueryParams<'_>) -> via::Result<Self> {
        let page = map_or(query.first("page"), 1i64, str::parse)?;

        if page < 1 {
            via::deny!(400, "page must be a positive integer");
        }

        let limit = map_or(query.first("limit"), MIN_PER_PAGE, str::parse)?;
        let offset = (page - 1).saturating_mul(limit);

        Ok(Self {
            limit_and_offset: LimitAndOffset { limit, offset },
        })
    }
}
