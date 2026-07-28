use diesel::dsl::{self as sql, IntoBoxed};
use diesel::expression::AsExpression;
use diesel::expression_methods::{BoolExpressionMethods, ExpressionMethods};
use diesel::pg::Pg;
use diesel::query_dsl::methods::{BoxedDsl, FilterDsl, LimitDsl};
use diesel::{Expression, QueryDsl, sql_types};
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use via::request::QueryParams;

use super::{Limit, Paginate};
use crate::id::{self, Id};

type DtzValueExpr = <OffsetDateTime as AsExpression<sql_types::Timestamptz>>::Expression;
type PkValueExpr = <Id as AsExpression<id::SqlType>>::Expression;

type AfterKeysetExpr<Dtz, Pk> = sql::Or<
    sql::Gt<Dtz, DtzValueExpr>,
    sql::And<sql::Eq<Dtz, DtzValueExpr>, sql::Gt<Pk, PkValueExpr>>,
>;

type BeforeKeysetExpr<Dtz, Pk> = sql::Or<
    sql::Lt<Dtz, DtzValueExpr>,
    sql::And<sql::Eq<Dtz, DtzValueExpr>, sql::Lt<Pk, PkValueExpr>>,
>;

#[derive(Debug)]
pub struct Keyset {
    after: bool,
    limit: Limit,
    value: Option<KeysetArgs>,
}

pub struct KeysetOf<Dtz, Pk> {
    source: KeysetSource<Dtz, Pk>,
    keyset: Keyset,
}

#[derive(Debug)]
enum InvalidKeyset {
    DateTime,
    Format,
    Id,
}

#[derive(Debug)]
struct KeysetArgs {
    dtz: OffsetDateTime,
    pk: Id,
}

struct KeysetSource<Dtz, Pk> {
    dtz: Dtz,
    pk: Pk,
}

fn after_keyset<Dtz, Pk>(
    source: &KeysetSource<Dtz, Pk>,
    binds: &KeysetArgs,
) -> AfterKeysetExpr<Dtz, Pk>
where
    Dtz: Expression<SqlType = sql_types::Timestamptz> + Copy + Send,
    Pk: Expression<SqlType = id::SqlType> + Copy + Send,
{
    source
        .dtz
        .gt(binds.dtz)
        .or(source.dtz.eq(binds.dtz).and(source.pk.gt(binds.pk)))
}

fn before_keyset<Dtz, Pk>(
    source: &KeysetSource<Dtz, Pk>,
    binds: &KeysetArgs,
) -> BeforeKeysetExpr<Dtz, Pk>
where
    Dtz: Expression<SqlType = sql_types::Timestamptz> + Copy + Send,
    Pk: Expression<SqlType = id::SqlType> + Copy + Send,
{
    source
        .dtz
        .lt(binds.dtz)
        .or(source.dtz.eq(binds.dtz).and(source.pk.lt(binds.pk)))
}

impl Keyset {
    pub fn of<Dtz, Pk>(self, dtz: Dtz, pk: Pk) -> KeysetOf<Dtz, Pk> {
        KeysetOf {
            source: KeysetSource { dtz, pk },
            keyset: self,
        }
    }
}

impl TryFrom<QueryParams<'_>> for Keyset {
    type Error = via::Error;

    fn try_from(query: QueryParams<'_>) -> Result<Self, Self::Error> {
        if let Some(after) = query
            .first("after")
            .percent_decode()
            .ok_and_then(str::parse)?
        {
            Ok(Self {
                after: true,
                limit: query.first("limit").try_into()?,
                value: Some(after),
            })
        } else {
            Ok(Self {
                after: false,
                limit: query.first("limit").try_into()?,
                value: query
                    .first("before")
                    .percent_decode()
                    .ok_and_then(str::parse)?,
            })
        }
    }
}

impl<T, Dtz, Pk> Paginate<KeysetOf<Dtz, Pk>> for T
where
    //
    // Convert the original query into a PostgreSQL-specific boxed
    // query. A boxed query retains the same type when filters and
    // limits are conditionally applied.
    //
    T: QueryDsl + BoxedDsl<'static, Pg>,
    //
    //
    //
    IntoBoxed<'static, T, Pg>: FilterDsl<AfterKeysetExpr<Dtz, Pk>, Output = IntoBoxed<'static, T, Pg>>
        + FilterDsl<BeforeKeysetExpr<Dtz, Pk>, Output = IntoBoxed<'static, T, Pg>>
        + LimitDsl<Output = IntoBoxed<'static, T, Pg>>,
    //
    // A timestamp column and primary key used as a stable
    // tiebreaker.
    //
    Dtz: Expression<SqlType = sql_types::Timestamptz> + Copy + Send,
    Pk: Expression<SqlType = id::SqlType> + Copy + Send,
{
    type Output = IntoBoxed<'static, T, Pg>;

    fn page(self, page: KeysetOf<Dtz, Pk>) -> Self::Output {
        let query = self.into_boxed::<Pg>();
        let keyset = &page.keyset;

        if let Some(binds) = keyset.value.as_ref() {
            if keyset.after {
                let predicate = after_keyset(&page.source, binds);
                query.limit(keyset.limit.value()).filter(predicate)
            } else {
                let predicate = before_keyset(&page.source, binds);
                query.limit(keyset.limit.value()).filter(predicate)
            }
        } else {
            query.limit(keyset.limit.value())
        }
    }
}

impl std::error::Error for InvalidKeyset {}

impl Display for InvalidKeyset {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::DateTime => write!(f, "invalid datetime in query param"),
            Self::Format => write!(f, "invalid keyset query param format"),
            Self::Id => write!(f, "invalid uuid in keyset query param"),
        }
    }
}

impl FromStr for KeysetArgs {
    type Err = InvalidKeyset;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let (dtz, pk) = input.split_once(',').ok_or(InvalidKeyset::Format)?;

        Ok(Self {
            dtz: OffsetDateTime::parse(dtz, &Rfc3339).or(Err(InvalidKeyset::DateTime))?,
            pk: pk.parse().or(Err(InvalidKeyset::Id))?,
        })
    }
}
