//! System reset.
//!
//! # References
//!
//! - Specification: [RISC-V SBI SRST extension](https://docs.riscv.org/reference/sbi/v3.0/ext-sys-reset.html) —
//!   reset types, reasons, and error semantics.

#![forbid(unsafe_code)]

use rustsbi::SbiRet;
use spin::Mutex;

use crate::driver::SunxiWdg;
use crate::driver::{
    P1Pmic, ResetBackend, ResetError, ResetReason, ResetRequest, ResetType, SifiveTestDevice,
    SysconPoweroff, SysconReboot,
};

/// SBI system-reset extension service.
#[derive(Default)]
pub struct SbiReset {
    backend: Backend,
}

/// Only the selected platform backend occupies storage. Syscon poweroff and
/// reboot may coexist, so their combined variant retains both peripherals.
#[derive(Default)]
enum Backend {
    #[default]
    None,
    SifiveTest(ResetAdapter<SifiveTestDevice>),
    SpacemitP1(ResetAdapter<P1Pmic>),
    SysconPoweroff(SysconPoweroff),
    SysconReboot(SysconReboot),
    Syscon {
        poweroff: SysconPoweroff,
        reboot: SysconReboot,
    },
    SunxiWdg(ResetAdapter<SunxiWdg>),
}

impl SbiReset {
    pub fn new(
        sifive_test: Option<SifiveTestDevice>,
        spacemit_p1_pmic: Option<P1Pmic>,
        syscon_poweroff: Option<SysconPoweroff>,
        syscon_reboot: Option<SysconReboot>,
        sunxi_wdg: Option<SunxiWdg>,
    ) -> Self {
        // Prefer a platform reset backend before the generic syscon devices.
        let backend = if let Some(device) = sifive_test {
            Backend::SifiveTest(ResetAdapter(Mutex::new(device)))
        } else if let Some(device) = spacemit_p1_pmic {
            Backend::SpacemitP1(ResetAdapter(Mutex::new(device)))
        } else if let Some(device) = sunxi_wdg {
            Backend::SunxiWdg(ResetAdapter(Mutex::new(device)))
        } else {
            match (syscon_poweroff, syscon_reboot) {
                (None, None) => Backend::None,
                (Some(poweroff), None) => Backend::SysconPoweroff(poweroff),
                (None, Some(reboot)) => Backend::SysconReboot(reboot),
                (Some(poweroff), Some(reboot)) => Backend::Syscon { poweroff, reboot },
            }
        };
        Self { backend }
    }
}

impl rustsbi::Reset for SbiReset {
    /// Function internal to macros. Do not use.
    #[doc(hidden)]
    #[inline]
    fn _rustsbi_probe(&self) -> usize {
        usize::from(!matches!(self.backend, Backend::None))
    }

    #[inline]
    fn system_reset(&self, reset_type: u32, reset_reason: u32) -> SbiRet {
        use rustsbi::spec::srst::{
            RESET_TYPE_COLD_REBOOT, RESET_TYPE_SHUTDOWN, RESET_TYPE_WARM_REBOOT,
        };

        match &self.backend {
            Backend::None => SbiRet::not_supported(),
            Backend::SifiveTest(device) => device.system_reset(reset_type, reset_reason),
            Backend::SpacemitP1(device) => device.system_reset(reset_type, reset_reason),
            Backend::SysconPoweroff(device) => device.system_reset(reset_type, reset_reason),
            Backend::SysconReboot(device) => device.system_reset(reset_type, reset_reason),
            Backend::Syscon { poweroff, reboot } => match reset_type {
                RESET_TYPE_SHUTDOWN => poweroff.system_reset(reset_type, reset_reason),
                RESET_TYPE_COLD_REBOOT | RESET_TYPE_WARM_REBOOT => {
                    reboot.system_reset(reset_type, reset_reason)
                }
                _ => SbiRet::invalid_param(),
            },
            Backend::SunxiWdg(device) => device.system_reset(reset_type, reset_reason),
        }
    }
}

/// Adapts the existing mutable reset backends to a shared SBI service.
struct ResetAdapter<D>(Mutex<D>);

impl<D: ResetBackend> rustsbi::Reset for ResetAdapter<D> {
    fn system_reset(&self, reset_type: u32, reset_reason: u32) -> SbiRet {
        let Some(req) = parse_request(reset_type, reset_reason) else {
            return SbiRet::invalid_param();
        };
        let mut device = self.0.lock();
        let Some(command) = device.prepare_reset(req) else {
            return SbiRet::invalid_param();
        };
        match device.system_reset(command) {
            ResetError::NotSupported => SbiRet::not_supported(),
            ResetError::Failed => SbiRet::failed(),
        }
    }
}

fn reset_syscon<D: ResetBackend<Request = ()>>(
    device: &D,
    reset_type: u32,
    reset_reason: u32,
    issue: impl FnOnce() -> ResetError,
) -> SbiRet {
    let Some(req) = parse_request(reset_type, reset_reason) else {
        return SbiRet::invalid_param();
    };
    if device.prepare_reset(req).is_none() {
        return SbiRet::invalid_param();
    }
    match issue() {
        ResetError::NotSupported => SbiRet::not_supported(),
        ResetError::Failed => SbiRet::failed(),
    }
}

impl rustsbi::Reset for SysconPoweroff {
    fn system_reset(&self, reset_type: u32, reset_reason: u32) -> SbiRet {
        reset_syscon(self, reset_type, reset_reason, || self.poweroff())
    }
}

impl rustsbi::Reset for SysconReboot {
    fn system_reset(&self, reset_type: u32, reset_reason: u32) -> SbiRet {
        reset_syscon(self, reset_type, reset_reason, || self.reboot())
    }
}

/// Validates both raw parameters before querying or invoking a reset backend.
fn parse_request(reset_type: u32, reset_reason: u32) -> Option<ResetRequest> {
    use rustsbi::spec::srst::{
        RESET_REASON_NO_REASON, RESET_REASON_SYSTEM_FAILURE, RESET_TYPE_COLD_REBOOT,
        RESET_TYPE_SHUTDOWN, RESET_TYPE_WARM_REBOOT,
    };

    let reset_type = match reset_type {
        RESET_TYPE_SHUTDOWN => ResetType::Shutdown,
        RESET_TYPE_COLD_REBOOT => ResetType::ColdReboot,
        RESET_TYPE_WARM_REBOOT => ResetType::WarmReboot,
        0xf000_0000..=0xffff_ffff => ResetType::VendorSpecific(reset_type),
        _ => return None,
    };
    let reset_reason = match reset_reason {
        RESET_REASON_NO_REASON => ResetReason::NoReason,
        RESET_REASON_SYSTEM_FAILURE => ResetReason::SystemFailure,
        0xe000_0000..=0xefff_ffff => ResetReason::SbiSpecific(reset_reason),
        0xf000_0000..=0xffff_ffff => ResetReason::VendorSpecific(reset_reason),
        _ => return None,
    };
    Some(ResetRequest {
        reset_type,
        reset_reason,
    })
}

#[allow(unused)]
pub fn fail() -> ! {
    use rustsbi::spec::srst::{
        EID_SRST, RESET_REASON_SYSTEM_FAILURE, RESET_TYPE_SHUTDOWN, SYSTEM_RESET,
    };

    if let Some(dispatcher) = super::SBI_DISPATCHER.get() {
        let ret = rustsbi::RustSBI::handle_ecall(
            dispatcher,
            EID_SRST,
            SYSTEM_RESET,
            [
                RESET_TYPE_SHUTDOWN as usize,
                RESET_REASON_SYSTEM_FAILURE as usize,
                0,
                0,
                0,
                0,
            ],
        );
        error!("System failure reset returned: {ret:?}");
    }
    crate::fail::stop()
}
