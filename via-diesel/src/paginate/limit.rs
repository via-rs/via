use diesel::expression::AsExpression;
use diesel::sql_types;
use via::{deny, request::params::QueryParam};

use super::PER_PAGE;

const MIN: i64 = 2;

pub const DEFAULT_MAX_LIMIT: i64 = 50;

#[derive(AsExpression, Debug)]
#[diesel(sql_type = sql_types::BigInt)]
pub struct Limit<const MAX: i64 = DEFAULT_MAX_LIMIT> {
    value: i64,
}

impl<const MAX: i64> Limit<MAX> {
    #[inline]
    pub(super) fn value(&self) -> i64 {
        self.value.min(MAX)
    }
}

impl<const MAX: i64> Default for Limit<MAX> {
    fn default() -> Self {
        Self {
            value: if MAX > PER_PAGE { PER_PAGE } else { MAX },
        }
    }
}

impl<const MAX: i64> TryFrom<QueryParam<'_, '_>> for Limit<MAX> {
    type Error = via::Error;

    fn try_from(param: QueryParam<'_, '_>) -> Result<Self, Self::Error> {
        match param.ok_and_then(str::parse)? {
            Some(value) if value > MAX => deny!(400, "limit exceeds maximum value of {}", MAX),
            Some(value) if value < MIN => deny!(400, "limit must be >= {}", MIN),
            Some(value) => Ok(Self { value }),
            None => Ok(Self::default()),
        }
    }
}
