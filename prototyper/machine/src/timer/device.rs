//! Physical device role behind the supervisor timer capability.

use super::TimerError;

/// A bound timer mechanism usable by every admitted hart.
///
/// Implementations retain all CSR or MMIO authority. The safe `Timer`
/// capability supplies only validated physical hart IDs and deadlines.
pub(crate) trait TimerDevice: Send + Sync {
    /// Prepares the calling hart before it can program supervisor deadlines.
    ///
    /// Shared devices that need no hart-local setup may use this default.
    fn prepare_current_hart(&self) -> Result<(), TimerError> {
        Ok(())
    }

    /// Reads the complete monotonically increasing architectural time value.
    fn read_time(&self) -> u64;

    /// Programs `deadline` for the admitted physical `hart_id`.
    ///
    /// Binding validation and hart admission must have completed before this
    /// infallible device operation becomes reachable.
    fn set_compare(&self, hart_id: usize, deadline: u64);

    /// Claims and services a timer interrupt owned by this device.
    ///
    /// Returns `true` only when the interrupt was recognized and completely
    /// handled. Devices that deliver no machine timer interrupt use the
    /// default `false` implementation.
    fn handle_interrupt(&self) -> bool {
        false
    }
}
