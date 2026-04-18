use std::convert::Infallible;

/// All of the predicates in self must match the input.
pub struct And<T>(T);

/// Map a predicate's error to a different type.
pub struct MapErr<F, T>(F, T);

/// Coerce a predicate to a boolean expression and negate it.
pub struct Not<T>(T);

/// One of the predicates in self must match the input.
pub struct Or<T>(T);

/// Project the input from one type to another for a predicate.
pub struct On<T, U>(T, U);

/// Require a predicate to match if the first matches.
pub struct When<T, U>(T, U);

/// Match any input.
pub struct Wildcard;

/// An inexpensive comparison operation that can fail with context.
pub trait Predicate<Input: ?Sized> {
    type Error<'a>
    where
        Self: 'a;

    fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>>;
}

// Macros adapted for our use case from the nom crate:
// https://github.com/rust-bakery/nom/blob/main/src/branch/mod.rs

macro_rules! and_impls(
    ($first:ident $second:ident $($id: ident)+) => (
        and_impls!(__impl $first $second; $($id)+);
    );
    (__impl $($current:ident)*; $head:ident $($id: ident)+) => (
        impl_and_predicate!($($current)*);
        and_impls!(__impl $($current)* $head; $($id)+);
    );
    (__impl $($current:ident)*; $head:ident) => (
        impl_and_predicate!($($current)*);
        impl_and_predicate!($($current)* $head);
    );
);

macro_rules! or_impls(
    ($first:ident $second:ident $($id: ident)+) => (
        or_impls!(__impl $first $second; $($id)+);
    );
    (__impl $($current:ident)*; $head:ident $($id: ident)+) => (
        impl_or_predicate!($($current)*);
        or_impls!(__impl $($current)* $head; $($id)+);
    );
    (__impl $($current:ident)*; $head:ident) => (
        impl_or_predicate!($($current)*);
        impl_or_predicate!($($current)* $head);
    );
);

macro_rules! impl_and_predicate {
    ($first:ident $($id:ident)+) => {
        impl<Input, $first, $($id),+> Predicate<Input> for ($first, $($id),+)
        where
            Input: ?Sized,
            for<'a> $first: Predicate<Input> + 'a,
            $(for<'a> $id: Predicate<Input, Error<'a> = $first::Error<'a>> + 'a),+
        {
            type Error<'a> = $first::Error<'a>;

            fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
                #[allow(non_snake_case)]
                let ($first, $($id),+) = self;
                $first.cmp(input)
                    $(.and_then(|_| $id.cmp(input)))+
            }
        }
    };
}

macro_rules! impl_or_predicate {
    ($first:ident $($id:ident)+) => {
        impl<Input, $first, $($id),+> Predicate<Input> for Or<($first, $($id),+)>
        where
            Input: ?Sized,
            for<'a> $first: Predicate<Input> + 'a,
            $(for<'a> $id: Predicate<Input, Error<'a> = $first::Error<'a>> + 'a),+
        {
            type Error<'a> = $first::Error<'a>;

            fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
                #[allow(non_snake_case)]
                let ($first, $($id),+) = &self.0;
                $first.cmp(input)
                    $(.or_else(|_| $id.cmp(input)))+
            }
        }
    };
}

/// Map the provided predicate's error to a different type. The lifetime
/// associated with the original error must be erased.
pub fn map_err<F, T>(f: F, predicate: T) -> MapErr<F, T> {
    MapErr(f, predicate)
}

/// Coerce `predicate` to a boolean expression and negate it.
pub fn not<T>(predicate: T) -> Not<T> {
    Not(predicate)
}

/// Project a field on the lhs before testing `predicate`.
pub fn on<T, U>(project: T, predicate: U) -> On<T, U> {
    On(project, predicate)
}

/// One of the predicates in `tuple` must match the input.
pub fn or<T>(tuple: T) -> Or<T> {
    Or(tuple)
}

/// Apply the second predicate when the first matches the input.
pub fn when<T, U>(first: T, second: U) -> When<T, U> {
    When(first, second)
}

/// Match any input.
pub fn wildcard() -> Wildcard {
    Wildcard
}

// The maximum length of a tuple is 10.
// This limits the best, worst case cyclomatic complexity to 20.

and_impls!(A B C D E F G H I J);
or_impls!(A B C D E F G H I J);

impl<E, F, T, Input> Predicate<Input> for MapErr<F, T>
where
    for<'a> F: Fn(T::Error<'_>) -> E + Copy + 'a,
    for<'a> T: Predicate<Input> + 'a,
    Input: ?Sized,
{
    type Error<'a> = E;

    fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
        self.1.cmp(input).map_err(&self.0)
    }
}

impl<T, Input> Predicate<Input> for Not<T>
where
    for<'a> T: Predicate<Input> + 'a,
    Input: ?Sized,
{
    type Error<'a> = ();

    fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
        if self.0.cmp(input).is_err() {
            Ok(())
        } else {
            Err(())
        }
    }
}

impl<T, U, Input, Project> Predicate<Input> for On<T, U>
where
    for<'a> T: Fn(&Input) -> &Project + Copy + 'a,
    for<'a> U: Predicate<Project> + 'a,
    Project: ?Sized,
    Input: ?Sized,
{
    type Error<'a> = U::Error<'a>;

    fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
        self.1.cmp((self.0)(input))
    }
}

impl<T, U, Input> Predicate<Input> for When<T, U>
where
    for<'a> T: Predicate<Input> + 'a,
    for<'a> U: Predicate<Input> + 'a,
    Input: ?Sized,
{
    type Error<'a> = U::Error<'a>;

    fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
        self.0.cmp(input).map_or(Ok(()), |_| self.1.cmp(input))
    }
}

impl<Input> Predicate<Input> for Wildcard
where
    Input: ?Sized,
{
    type Error<'a> = Infallible;

    fn cmp<'a>(&'a self, _: &Input) -> Result<(), Self::Error<'a>> {
        Ok(())
    }
}
