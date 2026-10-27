//! System-reset device class.
//!
//! Each enabled reset node is parsed by a class driver into a [`Description`];
//! binding acquires the register windows and returns one [`ResetDevice`]. The
//! syscon backend retains both poweroff and reboot actions. The SBI SRST adapter
//! owns the selected device and applies the class-table priority.

mod allwinner;
mod sifive;
mod spacemit;
mod syscon;

use alloc::boxed::Box;
use runtime::PlatformView;

use crate::devicetree::u32_property;
use crate::driver::device::{self, Class, Drivers};

/// The reset class.
pub(crate) struct Reset;

impl Class for Reset {
    type Device = Box<dyn ResetDevice>;
}

pub(crate) type Description = device::Description<Reset>;

pub(crate) static DRIVERS: Drivers<Reset> = Drivers::new(&[
    &sifive::TEST,
    &spacemit::P1_PMIC,
    &allwinner::WATCHDOG_V104,
    &allwinner::WATCHDOG_V105,
    &syscon::SYSCON,
]);

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

/// Low-level error category for an SRST device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResetError {
    /// The device cannot represent the requested operation.
    InvalidRequest,
    /// The reset failed for unspecified or unknown other reasons.
    Failed,
}

/// One reset device.
///
/// Validation and execution share one call because a successful reset never
/// returns; an unimplemented request must fail before any side effect.
pub(crate) trait ResetDevice: Send {
    /// Static selection priority; higher wins.
    fn priority(&self) -> u32 {
        0
    }

    /// Attempts the requested reset.
    fn system_reset(&mut self, request: ResetRequest) -> ResetError;
}

pub(super) fn timebase_frequency_hz(platform: &PlatformView<'_>) -> Option<u32> {
    let cpus = platform.find_enabled_node("/cpus")?;
    u32_property(cpus, "timebase-frequency").filter(|frequency| *frequency != 0)
}
