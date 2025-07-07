//! "Notification" functionality
//!
//! This functionality allows (or doesn't allow) for awaiting a read/write grant

use const_init::ConstInit;

#[cfg(feature = "maitake-sync-0_2")]
pub mod maitake;

pub mod blocking;

/// Non-async notifications
pub trait Notifier: ConstInit {
    fn wake_one_consumer(&self);
    fn wake_one_producer(&self);
}

/// Async notifications
#[allow(async_fn_in_trait)]
pub trait AsyncNotifier: Notifier {
    async fn wait_for_not_empty<T, F: FnMut() -> Option<T>>(&self, f: F) -> T;
    async fn wait_for_not_full<T, F: FnMut() -> Option<T>>(&self, f: F) -> T;
}
