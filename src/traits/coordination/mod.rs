#[cfg(feature = "cas-atomics")]
pub mod cas;

#[cfg(feature = "critical-section")]
pub mod cs;

/// Coordination Handler
///
/// The coordination handler is responsible for arbitrating access to the storage
///
/// # Safety
///
/// you must implement these correctly, or UB could happen
pub unsafe trait Coord {
    const INIT: Self;

    // Reset all EXCEPT taken values back to the initial empty state
    fn reset(&self);

    // Write Grants

    fn grant_max_remaining(&self, capacity: usize, sz: usize) -> Result<(usize, usize), ()>;
    fn grant_exact(&self, capacity: usize, sz: usize) -> Result<usize, ()>;
    fn commit_inner(&self, capacity: usize, grant_len: usize, used: usize);

    // Read Grants

    fn read(&self) -> Result<(usize, usize), ()>;
    fn release_inner(&self, used: usize);
}
