use super::ErrorKind;

pub struct And<T>(T);

pub struct Or<T>(T);

pub struct Not<T>(T);

pub struct When<T, U> {
    condition: T,
    predicate: U,
}

pub trait Predicate<Input: ?Sized> {
    fn cmp(&self, input: &Input) -> Result<(), ErrorKind>;

    fn matches(&self, input: &Input) -> bool {
        self.cmp(input).is_ok()
    }
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
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
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
            fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
                #[allow(non_snake_case)]
                let ($first, $($id),+) = &self.0;
                $first.cmp(input)
                    $(.or_else(|_| $id.cmp(input)))+
            }
        }
    };
}

pub fn and<T>(list: T) -> And<T> {
    And(list)
}

pub fn not<T>(predicate: T) -> Not<T> {
    Not(predicate)
}

pub fn or<T>(list: T) -> Or<T> {
    Or(list)
}

pub fn when<T, U>(condition: T, predicate: U) -> When<T, U> {
    When {
        condition,
        predicate,
    }
}

// The maximum length of a tuple is 20.
// This is the worst case cyclomatic complexity.

and_impls!(A B C D E F G H I J K L M N O P Q R S T);
or_impls!(A B C D E F G H I J K L M N O P Q R S T);

impl<Input, T> Predicate<Input> for Not<T>
where
    T: Predicate<Input>,
{
    fn cmp(&self, value: &Input) -> Result<(), ErrorKind> {
        if self.0.cmp(value).is_err() {
            Ok(())
        } else {
            Err(ErrorKind::Not)
        }
    }
}

impl<Input, T, U> Predicate<Input> for When<T, U>
where
    T: Predicate<Input>,
    U: Predicate<Input>,
{
    fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
        if self.condition.cmp(input).is_ok() {
            self.predicate.cmp(input)
        } else {
            Ok(())
        }
    }
}

impl<Input, T> Predicate<Input> for T
where
    T: Fn(&Input) -> Result<(), ErrorKind> + Copy,
{
    fn cmp(&self, input: &Input) -> Result<(), ErrorKind> {
        self(input)
    }
}
