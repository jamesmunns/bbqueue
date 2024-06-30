use core::{future::Future, pin};

use maitake_sync::{
    wait_cell::{Subscribe, Wait},
    WaitCell,
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

pub struct SubWrap<'a> {
    s: Subscribe<'a>,
}

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

pub struct WaitWrap<'a> {
    w: Wait<'a>,
}

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
