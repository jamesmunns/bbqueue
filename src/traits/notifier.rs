use core::future::Future;

#[cfg(feature = "maitake-sync-0_1")]
use core::pin;

#[cfg(feature = "maitake-sync-0_1")]
use maitake_sync::{
    wait_cell::{Subscribe, Wait},
    WaitCell,
};

pub trait Notifier {
    const INIT: Self;

    fn wake_one_consumer(&self);
    fn wake_one_producer(&self);
}

pub trait AsyncNotifier: Notifier {
    type NotEmptyRegisterFut<'a>: Future<Output = Self::NotEmptyWaiterFut<'a>>
    where
        Self: 'a;
    type NotFullRegisterFut<'a>: Future<Output = Self::NotFullWaiterFut<'a>>
    where
        Self: 'a;
    type NotEmptyWaiterFut<'a>: Future<Output = ()>
    where
        Self: 'a;
    type NotFullWaiterFut<'a>: Future<Output = ()>
    where
        Self: 'a;

    fn register_wait_not_empty(&self) -> Self::NotEmptyRegisterFut<'_>;
    fn register_wait_not_full(&self) -> Self::NotFullRegisterFut<'_>;
}

pub struct Blocking;

// Blocking performs no notification
impl Notifier for Blocking {
    const INIT: Self = Blocking;
    fn wake_one_consumer(&self) {}
    fn wake_one_producer(&self) {}
}

#[cfg(feature = "maitake-sync-0_1")]
pub struct MaiNotSpsc {
    not_empty: WaitCell,
    not_full: WaitCell,
}

#[cfg(feature = "maitake-sync-0_1")]
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

#[cfg(feature = "maitake-sync-0_1")]
pub struct SubWrap<'a> {
    s: Subscribe<'a>,
}

#[cfg(feature = "maitake-sync-0_1")]
impl<'a> Future for SubWrap<'a> {
    type Output = WaitWrap<'a>;

    fn poll(
        mut self: pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let pinned = pin::pin!(&mut self.s);
        pinned.poll(cx).map(|w| WaitWrap { w })
    }
}

#[cfg(feature = "maitake-sync-0_1")]
pub struct WaitWrap<'a> {
    w: Wait<'a>,
}

#[cfg(feature = "maitake-sync-0_1")]
impl<'a> Future for WaitWrap<'a> {
    type Output = ();

    fn poll(
        mut self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let pinned = pin::pin!(&mut self.w);
        pinned.poll(cx).map(drop)
    }
}

#[cfg(feature = "maitake-sync-0_1")]
impl AsyncNotifier for MaiNotSpsc {
    type NotEmptyRegisterFut<'a> = SubWrap<'a>;
    type NotFullRegisterFut<'a> = SubWrap<'a>;
    type NotEmptyWaiterFut<'a> = WaitWrap<'a>;
    type NotFullWaiterFut<'a> = WaitWrap<'a>;

    fn register_wait_not_empty(&self) -> Self::NotEmptyRegisterFut<'_> {
        SubWrap {
            s: self.not_empty.subscribe(),
        }
    }

    fn register_wait_not_full(&self) -> Self::NotFullRegisterFut<'_> {
        SubWrap {
            s: self.not_full.subscribe(),
        }
    }
}
