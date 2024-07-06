
use maitake_sync::{
    WaitCell, WaitQueue,
};

use super::{AsyncNotifier, Notifier};

pub struct MaiNotSpsc {
    not_empty: WaitCell,
    not_full: WaitCell,
}

impl MaiNotSpsc {
    pub fn new() -> Self {
        Self::INIT
    }
}

impl Default for MaiNotSpsc {
    fn default() -> Self {
        Self::new()
    }
}

impl Notifier for MaiNotSpsc {
    #[allow(clippy::declare_interior_mutable_const)]
    const INIT: Self = Self {
        not_empty: WaitCell::new(),
        not_full: WaitCell::new(),
    };

    fn wake_one_consumer(&self) {
        _ = self.not_empty.wake();
    }

    fn wake_one_producer(&self) {
        _ = self.not_full.wake();
    }
}

impl AsyncNotifier for MaiNotSpsc {
    async fn wait_for_not_empty<T, F: FnMut() -> Option<T>>(&self, f: F) -> T {
        self.not_empty.wait_for_value(f).await.unwrap()
    }

    async fn wait_for_not_full<T, F: FnMut() -> Option<T>>(&self, f: F) -> T {
        self.not_full.wait_for_value(f).await.unwrap()
    }
}

// ---

pub struct MaiNotMpsc {
    not_empty: WaitCell,
    not_full: WaitQueue,
}

impl MaiNotMpsc {
    pub fn new() -> Self {
        Self::INIT
    }
}

impl Default for MaiNotMpsc {
    fn default() -> Self {
        Self::new()
    }
}

impl Notifier for MaiNotMpsc {
    #[allow(clippy::declare_interior_mutable_const)]
    const INIT: Self = Self {
        not_empty: WaitCell::new(),
        not_full: WaitQueue::new(),
    };

    fn wake_one_consumer(&self) {
        _ = self.not_empty.wake();
    }

    fn wake_one_producer(&self) {
        self.not_full.wake();
    }
}

impl AsyncNotifier for MaiNotMpsc {
    async fn wait_for_not_empty<T, F: FnMut() -> Option<T>>(&self, f: F) -> T {
        self.not_empty.wait_for_value(f).await.unwrap()
    }

    async fn wait_for_not_full<T, F: FnMut() -> Option<T>>(&self, f: F) -> T {
        self.not_full.wait_for_value(f).await.unwrap()
    }
}
