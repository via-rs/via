use super::Deny;

pub struct And<T>(T);

pub struct Or<T>(T);

pub struct Not<T>(T);

pub struct Opt<T>(pub(super) T);

pub struct When<T, U>(T, U);

pub trait Predicate<Input: ?Sized> {
    fn cmp(&self, input: &Input) -> Result<(), Deny>;

    fn not(self) -> Not<Self>
    where
        Self: Sized,
    {
        Not(self)
    }

    fn optional(self) -> Opt<Self>
    where
        Self: Sized,
    {
        Opt(self)
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
    ($($id:ident)+) => {
        impl<Input, $($id),+> Predicate<Input> for And<($($id),+)>
        where
            $($id: Predicate<Input>),+
        {
            fn cmp(&self, input: &Input) -> Result<(), Deny> {
                #[allow(non_snake_case)]
                let ($($id),+) = &self.0;
                $($id.cmp(input)?;)+
                Ok(())
            }
        }
    };
}

macro_rules! impl_or_predicate {
    ($($id:ident)+) => {
        impl<Input, $($id),+> Predicate<Input> for Or<($($id),+)>
        where
            Input: ?Sized,
            $($id: Predicate<Input>),+
        {
            fn cmp(&self, input: &Input) -> Result<(), Deny> {
                #[allow(non_snake_case)]
                let ($($id),+) = &self.0;
                Err(Deny::Match)
                    $(.or_else(|_| $id.cmp(input)))+
            }
        }
    };
}

pub fn and<T>(list: T) -> And<T> {
    And(list)
}

pub fn or<T>(list: T) -> Or<T> {
    Or(list)
}

pub fn when<T, U>(precondition: T, predicate: U) -> When<T, U> {
    When(precondition, predicate)
}

// The maximum length of a tuple is 20.
// This is the worst case cyclomatic complexity.

and_impls!(A B C D E F G H I J K L M N O P Q R S T U);
or_impls!(A B C D E F G H I J K L M N O P Q R S T U);

impl<T> And<T> {
    pub fn predicate(&self) -> &T {
        &self.0
    }
}

impl<Input, T> Predicate<Input> for Not<T>
where
    T: Predicate<Input>,
{
    fn cmp(&self, value: &Input) -> Result<(), Deny> {
        if self.0.cmp(value).is_err() {
            Ok(())
        } else {
            Err(Deny::Not)
        }
    }
}

impl<Input, T, U> Predicate<Input> for When<T, U>
where
    T: Predicate<Input>,
    U: Predicate<Input>,
{
    fn cmp(&self, input: &Input) -> Result<(), Deny> {
        let Self(precondition, predicate) = self;

        if precondition.cmp(input).is_ok() {
            predicate.cmp(input)
        } else {
            Ok(())
        }
    }
}
