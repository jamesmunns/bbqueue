use super::Notifier;
use const_init::ConstInit;

pub struct Blocking;

// Blocking performs no notification
impl Notifier for Blocking {
    fn wake_one_consumer(&self) {}
    fn wake_one_producer(&self) {}
}

impl ConstInit for Blocking {
    const INIT: Self = Blocking;
}
