macro_rules! hurry {
    ($future:expr) => {
        tokio::task::coop::unconstrained($future).await
    };
}

pub(crate) use hurry;
