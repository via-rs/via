/// Call the provided closure once.
macro_rules! once {
    ($call:expr) => {
        static __1: std::sync::Once = std::sync::Once::new();
        __1.call_once(|| {
            print!("warn: a lossy size hint must be used for RequestBody. ");
            println!("usize::MAX exceeds u64::MAX on this platform.");
        });
    };
}

pub(crate) use once;
