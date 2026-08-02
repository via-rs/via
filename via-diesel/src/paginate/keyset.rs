use diesel::dsl::{self as sql, IntoBoxed};
use diesel::expression::{AsExpression, Expression};
use diesel::expression_methods::{BoolExpressionMethods, ExpressionMethods};
use diesel::query_dsl::methods::{BoxedDsl, FilterDsl, LimitDsl};
use diesel::{QueryDsl, sql_types};
use std::fmt::Display;
use std::str::FromStr;
use via::request::QueryParams;

use super::Paginate;
use super::limit::{DEFAULT_MAX_LIMIT, Limit};

#[cfg(feature = "postgres")]
type Db = diesel::pg::Pg;

#[cfg(feature = "mysql")]
type Db = diesel::mysql::Mysql;

#[cfg(feature = "sqlite")]
type Db = diesel::sqlite::Sqlite;

type PivotValueExpr<Pc, Pv> = <Pv as AsExpression<sql::SqlTypeOf<Pc>>>::Expression;
type TiebreakerValueExpr<Tc, Tv> = <Tv as AsExpression<sql::SqlTypeOf<Tc>>>::Expression;

type AfterKeyset<Pc, Tc, Pv, Tv> = sql::Or<
    sql::Gt<Pc, PivotValueExpr<Pc, Pv>>,
    sql::And<sql::Eq<Pc, PivotValueExpr<Pc, Pv>>, sql::Gt<Tc, TiebreakerValueExpr<Tc, Tv>>>,
>;

type BeforeKeyset<Pc, Tc, Pv, Tv> = sql::Or<
    sql::Lt<Pc, PivotValueExpr<Pc, Pv>>,
    sql::And<sql::Eq<Pc, PivotValueExpr<Pc, Pv>>, sql::Lt<Tc, TiebreakerValueExpr<Tc, Tv>>>,
>;

#[derive(Debug)]
pub struct Keyset<Pivot, Tiebreaker, const MAX: i64 = DEFAULT_MAX_LIMIT> {
    limit: Limit<MAX>,
    value: Option<KeysetArgs<Pivot, Tiebreaker>>,
}

pub struct KeysetOf<Pc, Tc, Pv, Tv, const MAX: i64> {
    lhs: (Pc, Tc),
    rhs: Keyset<Pv, Tv, MAX>,
}

#[derive(Debug)]
struct KeysetArgs<Pv, Tv> {
    after: bool,
    value: (Pv, Tv),
}

impl<Pv, Tv, const MAX: i64> Keyset<Pv, Tv, MAX> {
    pub fn of<Pc, Tc>(self, pivot: Pc, tiebreaker: Tc) -> KeysetOf<Pc, Tc, Pv, Tv, MAX> {
        KeysetOf {
            lhs: (pivot, tiebreaker),
            rhs: self,
        }
    }
}

impl<Pv, Tv> KeysetArgs<Pv, Tv>
where
    Pv: FromStr,
    Tv: FromStr,
    Pv::Err: Display,
    Tv::Err: Display,
{
    fn after(input: &str) -> via::Result<Self> {
        let mut args = input.parse::<Self>()?;
        args.after = true;
        Ok(args)
    }

    fn before(input: &str) -> via::Result<Self> {
        input.parse()
    }
}

impl<Pv, Tv> FromStr for KeysetArgs<Pv, Tv>
where
    Pv: FromStr,
    Tv: FromStr,
    Pv::Err: Display,
    Tv::Err: Display,
{
    type Err = via::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let Some((pivot, tiebreaker)) = input.split_once(',') else {
            via::deny!(
                400,
                "invalid keyset (format): expected a comma separated pair"
            );
        };

        let pivot = pivot
            .parse()
            .map_err(|e| via::err!(400, "invalid keyset (pivot): {}", e))?;

        let tiebreaker = tiebreaker
            .parse()
            .map_err(|e| via::err!(400, "invalid keyset (tiebreaker): {}", e))?;

        Ok(Self {
            after: false,
            value: (pivot, tiebreaker),
        })
    }
}

impl<Pv, Tv, const MAX: i64> TryFrom<QueryParams<'_>> for Keyset<Pv, Tv, MAX>
where
    Pv: FromStr,
    Tv: FromStr,
    Pv::Err: Display,
    Tv::Err: Display,
{
    type Error = via::Error;

    fn try_from(query: QueryParams<'_>) -> Result<Self, Self::Error> {
        let limit = query.first("limit").try_into()?;
        let mut value = query
            .first("before")
            .percent_decode()
            .ok_and_then(KeysetArgs::before)?;

        if value.is_none() {
            value = query
                .first("after")
                .percent_decode()
                .ok_and_then(KeysetArgs::after)?;
        }

        Ok(Self { limit, value })
    }
}

impl<Pc, Tc, Pv, Tv, const MAX: i64> KeysetOf<Pc, Tc, Pv, Tv, MAX> {
    fn limit(&self) -> i64 {
        self.rhs.limit.value()
    }

    fn rhs(&self) -> Option<&KeysetArgs<Pv, Tv>> {
        self.rhs.value.as_ref()
    }
}

#[cfg(any(feature = "postgres", feature = "mysql", feature = "sqlite"))]
impl<Src, Pc, Tc, Pv, Tv, const MAX: i64> Paginate<KeysetOf<Pc, Tc, Pv, Tv, MAX>> for Src
where
    //
    // `Src` implements `QueryDsl` and `LimitDsl<i64>`.
    //
    Src: QueryDsl + LimitDsl,
    //
    // The output of `Src::limit` implements both `QueryDsl` and `BoxedDsl` for
    // `DB`.
    //
    <Src as LimitDsl>::Output: QueryDsl + BoxedDsl<'static, Db>,
    //
    // The output of `QueryDsl::into_boxed` can be filtered by an `AfterKeyset`
    // expression or a `BeforeKeyset` expression.
    //
    IntoBoxed<'static, <Src as LimitDsl>::Output, Db>: FilterDsl<
            AfterKeyset<Pc, Tc, Pv, Tv>,
            Output = IntoBoxed<'static, <Src as LimitDsl>::Output, Db>,
        > + FilterDsl<
            BeforeKeyset<Pc, Tc, Pv, Tv>,
            Output = IntoBoxed<'static, <Src as LimitDsl>::Output, Db>,
        >,
    //
    // The pivot column implements `Expression` with `ExpressionMethods` and
    // when evaluated, it's `SqlType` is not null.
    //
    Pc: Expression + ExpressionMethods + Copy + Send,
    <Pc as Expression>::SqlType: sql_types::SqlType<IsNull = sql_types::is_nullable::NotNull>,
    //
    // The tiebreaker column implements `Expression` with `ExpressionMethods`
    // and when evaluated, it's `SqlType` is not null.
    //
    Tc: Expression + ExpressionMethods + Copy + Send,
    <Tc as Expression>::SqlType: sql_types::SqlType<IsNull = sql_types::is_nullable::NotNull>,
    //
    // The value expressions have the same SqlType as their respective columns
    // and implement `Copy + Send`.
    //
    Pv: AsExpression<sql::SqlTypeOf<Pc>> + Copy + Send,
    Tv: AsExpression<sql::SqlTypeOf<Tc>> + Copy + Send,
    //
    // The equality and comparison expressions on the pivot column and value
    // evaluate to a `sql_types::Bool`.
    //
    sql::Eq<Pc, Pv>: Expression<SqlType = sql_types::Bool>,
    sql::Gt<Pc, Pv>: Expression<SqlType = sql_types::Bool>,
    sql::Lt<Pc, Pv>: Expression<SqlType = sql_types::Bool>,
{
    type Output = IntoBoxed<'static, <Src as LimitDsl>::Output, Db>;

    fn page(self, keyset: KeysetOf<Pc, Tc, Pv, Tv, MAX>) -> Self::Output {
        let query = LimitDsl::limit(self, keyset.limit()).into_boxed::<Db>();

        if let Some(rhs) = keyset.rhs() {
            let lhs = &keyset.lhs;
            let (pv, tv) = &rhs.value;

            if rhs.after {
                query.filter(lhs.0.gt(*pv).or(lhs.0.eq(*pv).and(lhs.1.gt(*tv))))
            } else {
                query.filter(lhs.0.lt(*pv).or(lhs.0.eq(*pv).and(lhs.1.lt(*tv))))
            }
        } else {
            query
        }
    }
}
