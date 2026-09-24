use bitflags::bitflags;
use std::sync::atomic::{AtomicU8, Ordering};

pub(super) struct CancellationState {
    flags: AtomicU8,
}

bitflags! {
    #[derive(Eq, PartialEq)]
    pub(super) struct CancellationFlags: u8 {
        const WAITING = 0;
        const PANICKED = 1 << 0;
        const CANCELLED = 1 << 1;
    }
}

impl CancellationState {
    pub(super) fn new() -> Self {
        Self {
            flags: Default::default(),
        }
    }

    pub(super) fn cancel(&self) {
        self.insert(CancellationFlags::CANCELLED);
    }

    pub(super) fn panic(&self) {
        self.insert(CancellationFlags::CANCELLED | CancellationFlags::PANICKED);
    }

    pub(super) fn did_panic(&self) -> bool {
        self.has(CancellationFlags::PANICKED)
    }

    pub(super) fn is_waiting(&self) -> bool {
        self.flags.load(Ordering::Relaxed) == 0
    }

    #[inline]
    fn has(&self, state: CancellationFlags) -> bool {
        (self.flags.load(Ordering::Relaxed) & state.bits()) == state.bits()
    }

    #[inline]
    fn insert(&self, state: CancellationFlags) {
        self.flags.fetch_or(state.bits(), Ordering::Relaxed);
    }
}
