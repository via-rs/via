use diesel::dsl::{self as sql, IntoBoxed};
use diesel::expression::AsExpression;
use diesel::expression_methods::{BoolExpressionMethods, ExpressionMethods};
use diesel::pg::Pg;
use diesel::query_dsl::methods::{BoxedDsl, FilterDsl, LimitDsl};
use diesel::{Expression, QueryDsl, sql_types};
use std::fmt::Display;
use std::str::FromStr;
use via::request::QueryParams;

use super::Paginate;
use super::limit::{DEFAULT_MAX_LIMIT, Limit};

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

pub struct KeysetExpr<Pc, Tc, Pv, Tv, const MAX: i64> {
    lhs: (Pc, Tc),
    rhs: Keyset<Pv, Tv, MAX>,
}

#[derive(Debug)]
struct KeysetArgs<Pv, Tv> {
    after: bool,
    value: (Pv, Tv),
}

fn after<Pc, Tc, Pv, Tv>(lhs: &(Pc, Tc), rhs: &(Pv, Tv)) -> AfterKeyset<Pc, Tc, Pv, Tv>
where
    Pc: Expression + ExpressionMethods + Copy + Send,
    <Pc as Expression>::SqlType: sql_types::SqlType<IsNull = sql_types::is_nullable::NotNull>,
    //
    sql::Eq<Pc, Pv>: Expression<SqlType = sql_types::Bool>,
    sql::Gt<Pc, Pv>: Expression<SqlType = sql_types::Bool>,
    //
    Tc: Expression + ExpressionMethods + Copy + Send,
    <Tc as Expression>::SqlType: sql_types::SqlType<IsNull = sql_types::is_nullable::NotNull>,
    //
    Pv: AsExpression<sql::SqlTypeOf<Pc>> + Copy + Send,
    Tv: AsExpression<sql::SqlTypeOf<Tc>> + Copy + Send,
{
    lhs.0.gt(rhs.0).or(lhs.0.eq(rhs.0).and(lhs.1.gt(rhs.1)))
}

fn before<Pc, Tc, Pv, Tv>(lhs: &(Pc, Tc), rhs: &(Pv, Tv)) -> BeforeKeyset<Pc, Tc, Pv, Tv>
where
    Pc: Expression + ExpressionMethods + Copy + Send,
    <Pc as Expression>::SqlType: sql_types::SqlType<IsNull = sql_types::is_nullable::NotNull>,
    //
    sql::Eq<Pc, Pv>: Expression<SqlType = sql_types::Bool>,
    sql::Lt<Pc, Pv>: Expression<SqlType = sql_types::Bool>,
    //
    Tc: Expression + ExpressionMethods + Copy + Send,
    <Tc as Expression>::SqlType: sql_types::SqlType<IsNull = sql_types::is_nullable::NotNull>,
    //
    Pv: AsExpression<sql::SqlTypeOf<Pc>> + Copy + Send,
    Tv: AsExpression<sql::SqlTypeOf<Tc>> + Copy + Send,
{
    lhs.0.lt(rhs.0).or(lhs.0.eq(rhs.0).and(lhs.1.lt(rhs.1)))
}

impl<Pv, Tv, const MAX: i64> Keyset<Pv, Tv, MAX> {
    pub fn of<Pc, Tc>(self, pivot: Pc, tiebreaker: Tc) -> KeysetExpr<Pc, Tc, Pv, Tv, MAX> {
        KeysetExpr {
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

impl<Pc, Tc, Pv, Tv, const MAX: i64> KeysetExpr<Pc, Tc, Pv, Tv, MAX> {
    fn limit(&self) -> i64 {
        self.rhs.limit.value()
    }

    fn rhs(&self) -> Option<&KeysetArgs<Pv, Tv>> {
        self.rhs.value.as_ref()
    }
}

impl<Src, Pc, Tc, Pv, Tv, const MAX: i64> Paginate<KeysetExpr<Pc, Tc, Pv, Tv, MAX>> for Src
where
    Src: QueryDsl + BoxedDsl<'static, Pg>,
    //
    //
    //
    IntoBoxed<'static, Src, Pg>: FilterDsl<AfterKeyset<Pc, Tc, Pv, Tv>, Output = IntoBoxed<'static, Src, Pg>>
        + FilterDsl<BeforeKeyset<Pc, Tc, Pv, Tv>, Output = IntoBoxed<'static, Src, Pg>>
        + LimitDsl<Output = IntoBoxed<'static, Src, Pg>>,
    //
    // A timestampz column and primary key column are required.
    //
    Pc: Expression + ExpressionMethods + Copy + Send,
    <Pc as Expression>::SqlType: sql_types::SqlType<IsNull = sql_types::is_nullable::NotNull>,
    //
    sql::Eq<Pc, Pv>: Expression<SqlType = sql_types::Bool>,
    sql::Gt<Pc, Pv>: Expression<SqlType = sql_types::Bool>,
    sql::Lt<Pc, Pv>: Expression<SqlType = sql_types::Bool>,
    //
    Tc: Expression + ExpressionMethods + Copy + Send,
    <Tc as Expression>::SqlType: sql_types::SqlType<IsNull = sql_types::is_nullable::NotNull>,
    //
    Pv: AsExpression<sql::SqlTypeOf<Pc>> + Copy + Send,
    Tv: AsExpression<sql::SqlTypeOf<Tc>> + Copy + Send,
{
    type Output = IntoBoxed<'static, Src, Pg>;

    fn page(self, keyset: KeysetExpr<Pc, Tc, Pv, Tv, MAX>) -> Self::Output {
        let query = self.into_boxed::<Pg>();

        match keyset.rhs() {
            // After keyset
            Some(rhs) if rhs.after => query
                .limit(keyset.limit())
                .filter(after::<Pc, Tc, Pv, Tv>(&keyset.lhs, &rhs.value)),

            // Before keyset
            Some(rhs) => query
                .limit(keyset.limit())
                .filter(before::<Pc, Tc, Pv, Tv>(&keyset.lhs, &rhs.value)),

            // Empty keyset
            None => query.limit(keyset.limit()),
        }
    }
}
