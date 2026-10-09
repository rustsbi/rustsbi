//! System-reset firmware drivers.
//!
//! [`Description`] discovers reset hardware and selects a built-in
//! reset driver without acquiring MMIO. Binding produces a [`ResetController`],
//! which owns and serializes the selected [`ResetDevice`].

mod controller;
mod description;
mod pmic_spacemit_p1;
mod registry;
mod sifive_test;
mod sunxi_watchdog;
mod syscon;

pub(crate) use controller::ResetController;
pub(crate) use description::Description;

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

/// Failure of a reset device operation.
///
/// Invalid raw SBI values are rejected before device dispatch.
/// A successful reset request does not return.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResetError {
    /// The selected device cannot represent the requested operation.
    InvalidRequest,
    /// The reset failed for unspecified or unknown other reasons.
    ///
    /// Mapped to `SBI_ERR_FAILED`.
    Failed,
}

/// Operations supported by a selected reset device.
///
/// Validation and execution share one call because a successful reset never
/// returns; an unimplemented request must fail before any side effect.
pub(in crate::driver) trait ResetDevice: Send {
    /// Attempts the requested reset.
    fn system_reset(&mut self, request: ResetRequest) -> ResetError;
}
