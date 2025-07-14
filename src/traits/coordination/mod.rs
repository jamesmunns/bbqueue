//! "Coordination" functionality
//!
//! This trait is used to coordinate between Producers and Consumers.
//!
//! Unless you are on an embedded target without Compare and Swap atomics, e.g.
//! `cortex-m0`/`thumbv6m`, you almost certainly want to use the [`cas`] version
//! of coordination.
//!
//! The `cas` module is toggled automatically based on `#[cfg(target_has_atomic = "ptr")]`.

#[cfg(target_has_atomic = "ptr")]
pub mod cas;

#[cfg(feature = "critical-section")]
pub mod cs;

/// Errors associated with obtaining a write grant
#[derive(PartialEq, Debug)]
pub enum WriteGrantError {
    /// Unable to create write grant due to not enough room in the buffer
    InsufficientSize,
    /// Unable to create write grant due to existing write grant
    GrantInProgress,
}

/// Errors associated with obtaining a read grant
#[derive(PartialEq, Debug)]
pub enum ReadGrantError {
    /// Unable to create write grant due to not enough room in the buffer
    Empty,
    /// Unable to create write grant due to existing write grant
    GrantInProgress,
    /// We observed a frame header that did not make sense. This should only
    /// occur if a stream producer was used on one end and a frame consumer was
    /// used on the other end. Don't do that.
    ///
    /// If you see this error and you are NOT doing that, please report it, as it
    /// is a bug.
    InconsistentFrameHeader,
}

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

    fn grant_max_remaining(&self, capacity: usize, sz: usize) -> Result<(usize, usize), WriteGrantError>;
    fn grant_exact(&self, capacity: usize, sz: usize) -> Result<usize, WriteGrantError>;
    fn commit_inner(&self, capacity: usize, grant_len: usize, used: usize);

    // Read Grants

    fn read(&self) -> Result<(usize, usize), ReadGrantError>;
    fn release_inner(&self, used: usize);
}
