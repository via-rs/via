macro_rules! sealed {
    ($($impl:ident $(<$($args:ident),*>)?),+) => {
        mod sealed {
            /// A marker trait used to prevent external implementations.
            #[allow(dead_code)]
            pub trait Sealed {}
        }

        $(impl$(<$($args),*>)? sealed::Sealed for $impl $(<$($args),*>)? {})+
    };
}

pub(crate) use sealed;
