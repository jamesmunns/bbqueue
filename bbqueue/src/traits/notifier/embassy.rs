//! Async notifications using `embassy-sync` waker types

use core::future::poll_fn;
use core::task::Poll;

use const_init::ConstInit;
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, RawMutex};
use embassy_sync::waitqueue::GenericAtomicWaker;

use super::{AsyncNotifier, Notifier};

/// Async notifier for bbqueue based on `embassy-sync`'s [`GenericAtomicWaker`].
///
/// SPSC waiter, i.e. only one task can be registered to wait at a time, so it should generally only
/// be used with one task writing and one task reading.
pub struct EmbassyNotSpsc<M: RawMutex = CriticalSectionRawMutex> {
    not_empty: GenericAtomicWaker<M>,
    not_full: GenericAtomicWaker<M>,
}

impl<M: RawMutex> EmbassyNotSpsc<M> {
    /// Create a new `embassy-sync` based notifier
    pub const fn new() -> Self {
        Self::INIT
    }
}

impl<M: RawMutex> Default for EmbassyNotSpsc<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: RawMutex> ConstInit for EmbassyNotSpsc<M> {
    const INIT: Self = Self {
        not_empty: GenericAtomicWaker::new(M::INIT),
        not_full: GenericAtomicWaker::new(M::INIT),
    };
}

impl<M: RawMutex> Notifier for EmbassyNotSpsc<M> {
    fn wake_one_consumer(&self) {
        self.not_empty.wake();
    }

    fn wake_one_producer(&self) {
        self.not_full.wake()
    }
}

impl<M: RawMutex> AsyncNotifier for EmbassyNotSpsc<M> {
    async fn wait_for_not_empty<T, F: FnMut() -> Option<T>>(&self, mut f: F) -> T {
        poll_fn(move |cx| {
            // Register this task for wakeup before checking the function, avoiding missed wakeups
            // if another thread or irq were to try to wake between calls to f() and register().
            self.not_empty.register(cx.waker());
            f().map(Poll::Ready).unwrap_or(Poll::Pending)
        })
        .await
    }

    async fn wait_for_not_full<T, F: FnMut() -> Option<T>>(&self, mut f: F) -> T {
        poll_fn(move |cx| {
            self.not_full.register(cx.waker());
            f().map(Poll::Ready).unwrap_or(Poll::Pending)
        })
        .await
    }
}
