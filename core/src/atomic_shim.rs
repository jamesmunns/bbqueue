cfg_if::cfg_if! {
    if #[cfg(feature = "chaos")] {
        /// This is the "chaos" feature, which adds timing delays to atomic operations.
        /// It is only meant for stress-testing atomic ordering issues by inserting random delays.

        use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use rand::{thread_rng, Rng};
        use std::thread::sleep;
        use std::time::Duration;

        // TODO: Note for randoms:
        // * I don't want to bust cache on every atomic op
        // * How can I get "lazy" loading from env vars? Is this the right approach?
        // * These funcs can run from different threads
        // * Maybe just a thread local context with a basic rng and env vars?

        #[inline(always)]
        fn sleep_rand_nanos(min: u64, max:u64) {
            let mut rng = thread_rng();
            sleep(Duration::from_nanos(rng.gen_range(min, max)));
        }

        #[inline(always)]
        pub fn load(atomic: &AtomicUsize, order: Ordering) -> usize {
            sleep_rand_nanos(10, 10000); // before
            let ret = atomic.load(order);
            sleep_rand_nanos(10, 10000); // after
            ret
        }

        #[inline(always)]
        pub fn store(atomic: &AtomicUsize, val: usize, order: Ordering) {
            sleep_rand_nanos(10, 10000); // before
            atomic.store(val, order);
            sleep_rand_nanos(10, 10000); // after
        }

        #[inline(always)]
        pub fn load_bool(atomic: &AtomicBool, order: Ordering) -> bool {
            sleep_rand_nanos(10, 10000); // before
            let ret = atomic.load(order);
            sleep_rand_nanos(10, 10000); // after
            ret
        }

        #[inline(always)]
        pub fn store_bool(atomic: &AtomicBool, val: bool, order: Ordering) {
            sleep_rand_nanos(10, 10000); // before
            atomic.store(val, order);
            sleep_rand_nanos(10, 10000); // after
        }

        #[inline(always)]
        pub fn fetch_add(atomic: &AtomicUsize, val: usize, order: Ordering) -> usize {
            sleep_rand_nanos(10, 10000); // before
            let ret = atomic.fetch_add(val, order);
            sleep_rand_nanos(10, 10000); // after
            ret
        }

        #[inline(always)]
        pub fn fetch_sub(atomic: &AtomicUsize, val: usize, order: Ordering) -> usize {
            sleep_rand_nanos(10, 10000); // before
            let ret = atomic.fetch_sub(val, order);
            sleep_rand_nanos(10, 10000); // after
            ret
        }

        #[inline(always)]
        pub fn swap(atomic: &AtomicBool, val: bool, order: Ordering) -> bool {
            sleep_rand_nanos(10, 10000); // before
            let ret = atomic.swap(val, order);
            sleep_rand_nanos(10, 10000); // after
            ret
        }

    } else if #[cfg(not(feature = "thumbv6"))] {
        /// This is the "atomic" feature - use if your platform supports
        /// Atomic CAS operations

        use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

        #[inline(always)]
        pub fn load(atomic: &AtomicUsize, order: Ordering) -> usize {
            atomic.load(order)
        }

        #[inline(always)]
        pub fn store(atomic: &AtomicUsize, val: usize, order: Ordering) {
            atomic.store(val, order);
        }

        #[inline(always)]
        pub fn load_bool(atomic: &AtomicBool, order: Ordering) -> bool {
            atomic.load(order)
        }

        #[inline(always)]
        pub fn store_bool(atomic: &AtomicBool, val: bool, order: Ordering) {
            atomic.store(val, order);
        }

        #[inline(always)]
        pub fn fetch_add(atomic: &AtomicUsize, val: usize, order: Ordering) -> usize {
            atomic.fetch_add(val, order)
        }

        #[inline(always)]
        pub fn fetch_sub(atomic: &AtomicUsize, val: usize, order: Ordering) -> usize {
            atomic.fetch_sub(val, order)
        }

        #[inline(always)]
        pub fn swap(atomic: &AtomicBool, val: bool, order: Ordering) -> bool {
            atomic.swap(val, order)
        }

    } else if #[cfg(feature = "thumbv6")] {
        /// This is the "thumbv6" feature - use if you are on a Cortex-M0/M0+
        /// Atomic CAS operations are simulated with a critical section

        use core::sync::atomic::{
            AtomicBool, AtomicUsize,
            Ordering::{self, Acquire, Release},
        };
        use cortex_m::interrupt::free;

        #[inline(always)]
        pub fn load(atomic: &AtomicUsize, order: Ordering) -> usize {
            atomic.load(order)
        }

        #[inline(always)]
        pub fn store(atomic: &AtomicUsize, val: usize, order: Ordering) {
            atomic.store(val, order);
        }

        #[inline(always)]
        pub fn load_bool(atomic: &AtomicBool, order: Ordering) -> bool {
            atomic.load(order)
        }

        #[inline(always)]
        pub fn store_bool(atomic: &AtomicBool, val: bool, order: Ordering) {
            atomic.store(val, order);
        }

        #[inline(always)]
        pub fn fetch_add(atomic: &AtomicUsize, val: usize, _order: Ordering) -> usize {
            // TODO(AJM): This always models `_order == AcqRel`. Should we do a better
            // job of emulating the correct load and store orderings?
            free(|_| {
                let prev = atomic::load(&atomic, Acquire);
                store(atomic, prev.wrapping_add(val), Release);
                prev
            })
        }

        #[inline(always)]
        pub fn fetch_sub(atomic: &AtomicUsize, val: usize, _order: Ordering) -> usize {
            // TODO(AJM): This always models `_order == AcqRel`. Should we do a better
            // job of emulating the correct load and store orderings?
            free(|_| {
                let prev = atomic::load(&atomic, Acquire);
                store(atomic, prev.wrapping_sub(val), Release);
                prev
            })
        }

        #[inline(always)]
        pub fn swap(atomic: &AtomicBool, val: bool, _order: Ordering) -> bool {
            // TODO(AJM): This always models `_order == AcqRel`. Should we do a better
            // job of emulating the correct load and store orderings?
            free(|_| {
                let prev = atomic::load(&atomic, Acquire);
                atomic::store(&atomic, val, Release);
                prev
            })
        }
    } else {
        compile_error!("Bad Atomic Feature Selection!");
    }

}
