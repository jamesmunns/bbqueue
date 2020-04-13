cfg_if::cfg_if! {
    if #[cfg(not(feature = "thumbv6"))] {
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
