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

#[cfg(feature = "uuid")]
type PkSqlType = sql_types::Uuid;

#[cfg(not(feature = "uuid"))]
type PkSqlType = sql_types::BigInt;

type DtzValueExpr = <OffsetDateTime as AsExpression<sql_types::Timestamptz>>::Expression;
type PkValueExpr<T> = <T as AsExpression<PkSqlType>>::Expression;

type AfterKeysetExpr<T, Dtz, Pk> = sql::Or<
    sql::Gt<Dtz, DtzValueExpr>,
    sql::And<sql::Eq<Dtz, DtzValueExpr>, sql::Gt<Pk, PkValueExpr<T>>>,
>;

type BeforeKeysetExpr<T, Dtz, Pk> = sql::Or<
    sql::Lt<Dtz, DtzValueExpr>,
    sql::And<sql::Eq<Dtz, DtzValueExpr>, sql::Lt<Pk, PkValueExpr<T>>>,
>;

#[derive(Debug)]
pub struct Keyset<T> {
    limit: Limit,
    value: Option<KeysetArgs<T>>,
}

pub struct KeysetOf<T, Dtz, Pk> {
    of: KeysetSource<Dtz, Pk>,
    keyset: Keyset<T>,
}

#[derive(Debug)]
enum InvalidKeyset {
    DateTime,
    Format,
    Id,
}

#[derive(Debug)]
struct KeysetArgs<T> {
    after: bool,
    dtz: OffsetDateTime,
    pk: T,
}

struct KeysetSource<Dtz, Pk> {
    dtz: Dtz,
    pk: Pk,
}

fn after_keyset<T, Dtz, Pk>(
    source: &KeysetSource<Dtz, Pk>,
    binds: &KeysetArgs<T>,
) -> AfterKeysetExpr<T, Dtz, Pk>
where
    Dtz: Expression<SqlType = sql_types::Timestamptz> + Copy + Send,
    Pk: Expression<SqlType = PkSqlType> + Copy + Send,
    T: AsExpression<PkSqlType> + Copy + Send,
{
    source
        .dtz
        .gt(binds.dtz)
        .or(source.dtz.eq(binds.dtz).and(source.pk.gt(binds.pk)))
}

fn before_keyset<T, Dtz, Pk>(
    source: &KeysetSource<Dtz, Pk>,
    binds: &KeysetArgs<T>,
) -> BeforeKeysetExpr<T, Dtz, Pk>
where
    Dtz: Expression<SqlType = sql_types::Timestamptz> + Copy + Send,
    Pk: Expression<SqlType = PkSqlType> + Copy + Send,
    T: AsExpression<PkSqlType> + Copy + Send,
{
    source
        .dtz
        .lt(binds.dtz)
        .or(source.dtz.eq(binds.dtz).and(source.pk.lt(binds.pk)))
}

impl<T> Keyset<T> {
    pub fn of<Dtz, Pk>(self, dtz: Dtz, pk: Pk) -> KeysetOf<T, Dtz, Pk> {
        KeysetOf {
            of: KeysetSource { dtz, pk },
            keyset: self,
        }
    }
}

impl<T: FromStr> TryFrom<QueryParams<'_>> for Keyset<T> {
    type Error = via::Error;

    fn try_from(query: QueryParams<'_>) -> Result<Self, Self::Error> {
        if let Some(mut binds) = query
            .first("after")
            .percent_decode()
            .ok_and_then::<_, KeysetArgs<T>, _>(str::parse)?
        {
            binds.after = true;
            Ok(Self {
                limit: query.first("limit").try_into()?,
                value: Some(binds),
            })
        } else {
            Ok(Self {
                limit: query.first("limit").try_into()?,
                value: query
                    .first("before")
                    .percent_decode()
                    .ok_and_then(str::parse)?,
            })
        }
    }
}

impl<T, Dtz, Pk> KeysetOf<T, Dtz, Pk> {
    fn binds(&self) -> Option<&KeysetArgs<T>> {
        self.keyset().value.as_ref()
    }

    fn limit(&self) -> i64 {
        self.keyset().limit.value()
    }

    fn keyset(&self) -> &Keyset<T> {
        &self.keyset
    }
}

impl<Src, T, Dtz, Pk> Paginate<KeysetOf<T, Dtz, Pk>> for Src
where
    Src: QueryDsl + BoxedDsl<'static, Pg>,
    //
    //
    //
    IntoBoxed<'static, Src, Pg>: FilterDsl<AfterKeysetExpr<T, Dtz, Pk>, Output = IntoBoxed<'static, Src, Pg>>
        + FilterDsl<BeforeKeysetExpr<T, Dtz, Pk>, Output = IntoBoxed<'static, Src, Pg>>
        + LimitDsl<Output = IntoBoxed<'static, Src, Pg>>,
    //
    // A timestampz column and primary key column are required.
    //
    Dtz: Expression<SqlType = sql_types::Timestamptz> + Copy + Send,
    Pk: Expression<SqlType = PkSqlType> + Copy + Send,
    T: AsExpression<PkSqlType> + Copy + Send,
{
    type Output = IntoBoxed<'static, Src, Pg>;

    fn page(self, keyset: KeysetOf<T, Dtz, Pk>) -> Self::Output {
        let query = self.into_boxed::<Pg>();

        match keyset.binds() {
            // After keyset
            Some(binds) if binds.after => query
                .limit(keyset.limit())
                .filter(after_keyset(&keyset.of, binds)),

            // Before keyset
            Some(binds) => query
                .limit(keyset.limit())
                .filter(before_keyset(&keyset.of, binds)),

            // Empty keyset
            None => query.limit(keyset.limit()),
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

impl<T: FromStr> FromStr for KeysetArgs<T> {
    type Err = InvalidKeyset;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let (dtz, pk) = input.split_once(',').ok_or(InvalidKeyset::Format)?;

        Ok(Self {
            after: false,
            dtz: OffsetDateTime::parse(dtz, &Rfc3339).or(Err(InvalidKeyset::DateTime))?,
            pk: pk.parse().or(Err(InvalidKeyset::Id))?,
        })
    }
}
