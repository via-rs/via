/// Call the provided closure once.
macro_rules! once {
    ($call:expr) => {
        static __1: std::sync::Once = std::sync::Once::new();
        __1.call_once($call);
    };
}

pub(crate) use once;
