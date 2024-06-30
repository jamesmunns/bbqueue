use core::future::Future;

#[cfg(feature = "maitake-sync-0_1")]
pub mod maitake;

pub mod blocking;

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
