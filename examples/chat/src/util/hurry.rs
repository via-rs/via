macro_rules! hurry {
    ($future:expr) => {
        coop::unconstrained($future).await
    };
}

pub(crate) use hurry;
