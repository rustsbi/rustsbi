//! System-reset firmware drivers.
//!
//! [`Description`] discovers reset hardware and selects a registered
//! reset driver without acquiring MMIO. Binding produces one [`ResetDevice`],
//! which owns and serializes the selected backend for the SBI SRST adapter.

mod description;
mod device;
mod pmic_spacemit_p1;
mod registry;
mod sifive_test;
mod sunxi_watchdog;
mod syscon;

pub(crate) use description::Description;
pub(crate) use device::ResetDevice;

/// Parsed reset type accepted by the SRST driver layer.
///
/// Reserved raw values are intentionally not representable here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResetType {
    /// SBI-defined reset type `0x00000000`.
    Shutdown,
    /// SBI-defined reset type `0x00000001`.
    ColdReboot,
    /// SBI-defined reset type `0x00000002`.
    WarmReboot,
    /// Vendor- or platform-specific reset type: `0xF0000000..=0xFFFFFFFF`.
    VendorSpecific(u32),
}

/// Parsed reset reason accepted by the SRST driver layer.
///
/// Reserved raw values are intentionally not representable here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResetReason {
    /// SBI-defined reset reason `0x00000000`.
    NoReason,
    /// SBI-defined reset reason `0x00000001`.
    SystemFailure,
    /// SBI implementation-specific reset reason: `0xE0000000..=0xEFFFFFFF`.
    SbiSpecific(u32),
    /// Vendor- or platform-specific reset reason: `0xF0000000..=0xFFFFFFFF`.
    VendorSpecific(u32),
}

/// Fully parsed SRST request.
///
/// This is the unit consumed by the driver layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ResetRequest {
    reset_type: ResetType,
    reset_reason: ResetReason,
}

impl ResetRequest {
    pub(crate) const fn new(reset_type: ResetType, reset_reason: ResetReason) -> Self {
        Self {
            reset_type,
            reset_reason,
        }
    }

    const fn reset_type(self) -> ResetType {
        self.reset_type
    }

    const fn reset_reason(self) -> ResetReason {
        self.reset_reason
    }
}

/// Low-level error category for an SRST backend.
///
/// # Semantics
///
/// - `InvalidParam` is intentionally absent.
/// - Successful reset is intentionally absent too, because a successful
///   SRST request does not return.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResetError {
    /// The selected device cannot represent the requested operation.
    InvalidRequest,
    /// The reset failed for unspecified or unknown other reasons.
    ///
    /// Mapped to `SBI_ERR_FAILED`.
    Failed,
}

/// Behavior required of a selected reset backend.
///
/// Validation and execution share one call because a successful reset never
/// returns; an unimplemented request must fail before any side effect.
pub(in crate::driver) trait ResetBackend: Send {
    /// Attempts the requested reset.
    fn system_reset(&mut self, request: ResetRequest) -> ResetError;
}
