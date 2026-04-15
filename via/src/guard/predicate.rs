use super::GuardError;

pub struct And<T>(T);

pub struct Not<T>(T);

pub struct Or<T>(T);

pub struct On<T, U> {
    on: T,
    pred: U,
}

pub struct When<T, U> {
    cond: T,
    pred: U,
}

pub trait Predicate<Input: ?Sized> {
    fn cmp<'a>(&'a self, input: &Input) -> Result<(), GuardError<'a>>;
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
        impl<Input, $first, $($id),+> Predicate<Input> for And<($first, $($id),+)>
        where
            $first: Predicate<Input>,
            $($id: Predicate<Input>),+
        {
            fn cmp<'a>(&'a self, input: &Input) -> Result<(), GuardError<'a>> {
                #[allow(non_snake_case)]
                let ($first, $($id),+) = &self.0;
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
            $first: Predicate<Input>,
            $($id: Predicate<Input>),+
        {
            fn cmp<'a>(&'a self, input: &Input) -> Result<(), GuardError<'a>> {
                #[allow(non_snake_case)]
                let ($first, $($id),+) = &self.0;
                $first.cmp(input)
                    $(.or_else(|_| $id.cmp(input)))+
            }
        }
    };
}

pub fn and<T>(tuple: T) -> And<T> {
    And(tuple)
}

pub fn not<T>(pred: T) -> Not<T> {
    Not(pred)
}

pub fn on<T, U>(on: T, pred: U) -> On<T, U> {
    On { on, pred }
}

pub fn or<T>(tuple: T) -> Or<T> {
    Or(tuple)
}

pub fn when<T, U>(cond: T, pred: U) -> When<T, U> {
    When { cond, pred }
}

// The maximum length of a tuple is 20.
// This is the worst case cyclomatic complexity.

and_impls!(A B C D E F G H I J K L M N O P Q R S T);
or_impls!(A B C D E F G H I J K L M N O P Q R S T);

impl<Input, T> Predicate<Input> for Not<T>
where
    T: Predicate<Input>,
{
    fn cmp<'a>(&'a self, input: &Input) -> Result<(), GuardError<'a>> {
        if self.0.cmp(input).is_err() {
            Ok(())
        } else {
            Err(GuardError::Not)
        }
    }
}

impl<Input, Project, T, U> Predicate<Input> for On<T, U>
where
    T: Fn(&Input) -> &Project + Copy,
    U: Predicate<Project>,
{
    fn cmp<'a>(&'a self, input: &Input) -> Result<(), GuardError<'a>> {
        let projection = (self.on)(input);
        self.pred.cmp(projection)
    }
}

impl<Input, T, U> Predicate<Input> for When<T, U>
where
    T: Predicate<Input>,
    U: Predicate<Input>,
{
    fn cmp<'a>(&'a self, input: &Input) -> Result<(), GuardError<'a>> {
        self.cond
            .cmp(input)
            .map_or(Ok(()), |_| self.pred.cmp(input))
    }
}
