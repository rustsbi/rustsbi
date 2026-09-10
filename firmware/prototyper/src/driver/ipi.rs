//! Backend operations on ordinary SBI hart-mask windows.

/// One ordinary sPI hart-mask window request forwarded to the backend.
///
/// This backend models only the normal `(hart_mask, hart_mask_base)` window
/// form of the SBI hart-mask encoding.
///
/// The SBI special encoding `hart_mask_base == usize::MAX` means "ignore
/// hart_mask and target all available harts"; the SBI adaptation layer expands
/// it into ordinary window requests before calling the backend.
///
/// One call selects at most XLEN harts, matching MSWI / CLINT hardware
/// provisioned for the actual hart count rather than a specification maximum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IpiRequest {
    pub hart_mask: usize,
    pub hart_mask_base: usize,
}

impl IpiRequest {
    /// Iterates the targets of an already validated ordinary window.
    pub(crate) fn harts(self) -> impl Iterator<Item = usize> {
        sbi_spec::binary::HartMask::from_mask_base(self.hart_mask, self.hart_mask_base).into_iter()
    }
}

/// Low-level error category for one sPI backend operation.
///
/// Important:
/// - `InvalidParam` is intentionally absent.
///   Raw `(hart_mask, hart_mask_base)` decoding and target validation belong to
///   the SBI adaptation layer.
/// - Requests reaching this backend have already passed SBI target validation.
/// - The backend therefore only reports unspecified operational failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IpiError {
    /// Mapped by the SBI adaptation layer to `SBI_ERR_FAILED`.
    Failed,
}

/// A backend that implements one ordinary backend-side sPI send operation.
///
/// This operation follows SBI decoding and validation: `InvalidParam` and the
/// special "all available harts" encoding are not backend-visible outcomes or
/// request forms; higher layers split all-hart targets into ordinary windows.
///
/// Calls may run concurrently on different harts. Implementations must
/// synchronize any mutable software state internally; independent MMIO
/// writes and hart-local interrupt claims do not require a global lock.
pub trait IpiBackend {
    /// Sends supervisor IPIs to the targets of one ordinary request window.
    ///
    /// Returns `Ok(())` if all targets were signaled, or `Err(IpiError::Failed)`
    /// for an unspecified or unknown operational failure; some targets may
    /// already have been signaled when an error is returned.
    ///
    /// Requests have already been decoded, validated, and normalized by the
    /// SBI adaptation layer, including expansion of "all available harts".
    fn send_ipi(&self, req: IpiRequest) -> Result<(), IpiError>;

    /// *Internal function* that clears the firmware IPI pending register for
    /// the specified hart.
    ///
    /// This is an internal firmware operation, not an SBI function; it does
    /// not clear the supervisor's SSIP bit or software event bookkeeping.
    /// Returns `Failed` if the register cannot be cleared, including when a
    /// backend can only clear the calling hart and a remote hart was requested.
    fn clear_ipi(&self, hart_id: usize) -> Result<(), IpiError>;

    /// *Internal function* that reports whether firmware IPIs arrive through
    /// an IMSIC interrupt file.
    ///
    /// This internal property selects the firmware interrupt handling path.
    fn is_imsic(&self) -> bool {
        false
    }
}
