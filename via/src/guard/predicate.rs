pub struct And<T>(T);

pub struct MapErr<F, T>(F, T);

pub struct Not<T>(T);

pub struct Or<T>(T);

pub struct On<T, U>(T, U);

pub struct When<T, U>(T, U);

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

pub fn map_err<F, T>(f: F, predicate: T) -> MapErr<F, T> {
    MapErr(f, predicate)
}

pub fn not<T>(predicate: T) -> Not<T> {
    Not(predicate)
}

pub fn on<T, U>(project: T, predicate: U) -> On<T, U> {
    On(project, predicate)
}

pub fn or<T>(tuple: T) -> Or<T> {
    Or(tuple)
}

pub fn when<T, U>(condition: T, predicate: U) -> When<T, U> {
    When(condition, predicate)
}

// The maximum length of a tuple is 10.
// This limits the best, worst case cyclomatic complexity to 20.

and_impls!(A B C D E F G H I J);
or_impls!(A B C D E F G H I J);

impl<Input, Error, F, T> Predicate<Input> for MapErr<F, T>
where
    for<'a> F: Fn(T::Error<'_>) -> Error + Copy + 'a,
    for<'a> T: Predicate<Input> + 'a,
{
    type Error<'a> = Error;

    fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
        self.1.cmp(input).map_err(|error| (self.0)(error))
    }
}

impl<Input, T> Predicate<Input> for Not<T>
where
    for<'a> T: Predicate<Input> + 'a,
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

impl<Input, Project, T, U> Predicate<Input> for On<T, U>
where
    for<'a> T: Fn(&Input) -> &Project + Copy + 'a,
    for<'a> U: Predicate<Project> + 'a,
{
    type Error<'a> = U::Error<'a>;

    fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
        self.1.cmp((self.0)(input))
    }
}

impl<Input, T, U> Predicate<Input> for When<T, U>
where
    for<'a> T: Predicate<Input> + 'a,
    for<'a> U: Predicate<Input> + 'a,
{
    type Error<'a> = U::Error<'a>;

    fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
        self.0.cmp(input).map_or(Ok(()), |_| self.1.cmp(input))
    }
}
