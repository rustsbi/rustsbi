//! Platform operations transferred to Runtime during boot.

use crate::hart::HartId;

/// Native machine-timer operations implemented by a platform driver.
///
/// Drivers own validated MMIO, split writes, and shared-register
/// synchronization.
/// Counter reads remain shared, independent of comparator ownership.
/// These operations do not manage MTIE, STIP, or interrupt dispatch.
pub trait TimerDevice: Send + Sync {
    /// Reads the full device counter when available.
    fn read_time(&self) -> Option<u64> {
        None
    }
    /// Reads a direct counter word without probing architectural time.
    fn read_time_low(&self) -> Option<usize> {
        None
    }
    /// Reads a direct high counter word on RV32.
    #[cfg(target_pointer_width = "32")]
    fn read_time_high(&self) -> Option<usize> {
        None
    }

    /// Programs one comparator.
    /// `u64::MAX` cancels its deadline.
    /// Split writes must avoid a transient early interrupt.
    fn set_compare(&self, hart: HartId, deadline: u64);
    /// Acknowledges a source after Runtime masks MTIE.
    /// Devices needing only MTIE masking may override the default cancellation.
    fn acknowledge(&self, hart: HartId) {
        self.set_compare(hart, u64::MAX);
    }
}
