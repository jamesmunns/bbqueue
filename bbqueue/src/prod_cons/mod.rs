//! Producer and Consumer interfaces
//!
//! BBQueues can be used with one of two kinds of producer/consumer pairs:
//!
//! * **Framed**, where the consumer sees the exact chunks that were inserted by the
//!   producer. This uses a small length header to note the length of the inserted frame.
//!   This means that if the producer writes a 10 byte grant, a 20 byte grant, then a 30
//!   byte grant, the consumer will need to read three times to drain the queue, seeing the
//!   10, 20, and 30 byte chunks in order. This is useful when you are working with data that
//!   has logical "frames", for example for network packets.
//! * **Stream**, where the consumer may potentially see multiple pushed chunks at once, with
//!   no separation. This means that if the producer writes a 10 byte grant, a 20 byte grant,
//!   then a 30 byte grant, the consumer could potentially see all 60 bytes in a single read
//!   grant (if there is no wrap-around).
//!
//! You should NOT "mix and match" framed/stream consumers and producers. This will not cause
//! memory safety/UB issues, but will not work properly.

pub mod framed;
pub mod stream;

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use const_init::ConstInit;

    use crate::{
        BBQueue,
        traits::{
            coordination::cas::AtomicCoord,
            notifier::{AsyncNotifier, Notifier},
            storage::Inline,
        },
    };

    static WAIT_CALLS: AtomicUsize = AtomicUsize::new(0);

    struct CountingNotifier;

    impl ConstInit for CountingNotifier {
        const INIT: Self = Self;
    }

    impl Notifier for CountingNotifier {
        fn wake_one_consumer(&self) {}

        fn wake_one_producer(&self) {}
    }

    impl AsyncNotifier for CountingNotifier {
        async fn wait_for_not_empty<T, F: FnMut() -> Option<T>>(&self, mut f: F) -> T {
            WAIT_CALLS.fetch_add(1, Ordering::Relaxed);
            f().expect("test queue has readable data")
        }

        async fn wait_for_not_full<T, F: FnMut() -> Option<T>>(&self, mut f: F) -> T {
            WAIT_CALLS.fetch_add(1, Ordering::Relaxed);
            f().expect("test queue has writable space")
        }
    }

    #[tokio::test]
    async fn ready_waits_do_not_arm_notifier() {
        WAIT_CALLS.store(0, Ordering::Relaxed);

        let stream: BBQueue<Inline<64>, AtomicCoord, CountingNotifier> = BBQueue::new();
        let stream_producer = stream.stream_producer();
        let stream_consumer = stream.stream_consumer();

        stream_producer
            .wait_grant_exact(1)
            .await
            .expect("1 byte fits a 64 byte buffer")
            .commit(1);
        stream_consumer.wait_read().await.release(1);
        stream_producer.wait_grant_max_remaining(1).await.commit(1);
        stream_consumer.wait_read().await.release(1);

        let framed: BBQueue<Inline<64>, AtomicCoord, CountingNotifier> = BBQueue::new();
        let framed_producer = framed.framed_producer();
        let framed_consumer = framed.framed_consumer();

        framed_producer
            .wait_grant(1)
            .await
            .expect("a 1 byte frame fits a 64 byte buffer")
            .commit(1);
        framed_consumer.wait_read().await.release();

        assert_eq!(WAIT_CALLS.load(Ordering::Relaxed), 0);
    }
}
