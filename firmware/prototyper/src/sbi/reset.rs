#![forbid(unsafe_code)]

use alloc::boxed::Box;
use rustsbi::SbiRet;
use spin::Mutex;

use crate::platform::BoardInfo;
use crate::platform::reset::{P1PmicResetWrap, SifiveTestDeviceWrap};

/// RPMI System Reset service and reset-type identifiers.
mod rpmi_sysrst {
    pub const SERVICE_GET_ATTRIBUTES: u8 = 0x02;
    pub const SERVICE_SYSTEM_RESET: u8 = 0x03;
    // K3 management firmware reports reset-type support in bit 1.
    pub const ATTRIBUTE_SUPPORTED: u32 = 1 << 1;
    pub const TYPE_SHUTDOWN: u8 = 0x0;
    pub const TYPE_COLD_REBOOT: u8 = 0x1;
    pub const TYPE_WARM_REBOOT: u8 = 0x2;
}

/// K3 system reset device backed by the RPMI System Reset service group.
pub(crate) struct RpmiResetWrap {
    mailbox: &'static crate::rpmi::RpmiMailbox,
    warm_reset: bool,
}

impl RpmiResetWrap {
    fn new(mailbox: &'static crate::rpmi::RpmiMailbox) -> Self {
        let request = (rpmi_sysrst::TYPE_WARM_REBOOT as u32).to_le_bytes();
        let mut response = [0u8; 8];
        let warm_reset = matches!(
            mailbox.normal_request_with_status(
                crate::rpmi::servicegroup::SYSTEM_RESET,
                rpmi_sysrst::SERVICE_GET_ATTRIBUTES,
                &request,
                &mut response,
            ),
            Ok((::rpmi::message::Status::Success, len))
                if len >= response.len()
                    && u32::from_le_bytes([response[4], response[5], response[6], response[7]])
                        & rpmi_sysrst::ATTRIBUTE_SUPPORTED
                        != 0
        );
        Self {
            mailbox,
            warm_reset,
        }
    }

    fn do_reset(&self, reset_type: u8) -> ! {
        let req = (reset_type as u32).to_le_bytes();
        let _ = self.mailbox.posted_request(
            crate::rpmi::servicegroup::SYSTEM_RESET,
            rpmi_sysrst::SERVICE_SYSTEM_RESET,
            &req,
        );
        // Reset requests do not return; wait if the transport fails.
        loop {
            core::hint::spin_loop()
        }
    }
}

impl ResetDevice for RpmiResetWrap {
    fn fail(&self, _code: u16) -> ! {
        self.do_reset(rpmi_sysrst::TYPE_SHUTDOWN)
    }
    fn pass(&self) -> ! {
        self.do_reset(rpmi_sysrst::TYPE_SHUTDOWN)
    }
    fn supports_warm_reset(&self) -> bool {
        self.warm_reset
    }
    fn reset(&self, warm: bool) -> ! {
        let reset_type = if warm {
            rpmi_sysrst::TYPE_WARM_REBOOT
        } else {
            rpmi_sysrst::TYPE_COLD_REBOOT
        };
        self.do_reset(reset_type)
    }
}

pub trait ResetDevice: Send {
    fn fail(&self, code: u16) -> !;
    fn pass(&self) -> !;
    fn supports_warm_reset(&self) -> bool {
        false
    }
    fn reset(&self, warm: bool) -> !;
}

pub struct SbiReset {
    pub reset_dev: Mutex<Box<dyn ResetDevice>>,
}

impl SbiReset {
    pub fn new(reset_dev: Mutex<Box<dyn ResetDevice>>) -> Self {
        Self { reset_dev }
    }

    #[allow(unused)]
    pub fn fail(&self) -> ! {
        trace!("Test fail, invoke process exit procedure on Reset device");
        self.reset_dev.lock().fail(0);
    }
}

impl rustsbi::Reset for SbiReset {
    #[inline]
    fn system_reset(&self, reset_type: u32, reset_reason: u32) -> SbiRet {
        use rustsbi::spec::srst::{
            RESET_REASON_NO_REASON, RESET_REASON_SYSTEM_FAILURE, RESET_TYPE_COLD_REBOOT,
            RESET_TYPE_SHUTDOWN, RESET_TYPE_WARM_REBOOT,
        };
        if !matches!(
            reset_reason,
            RESET_REASON_NO_REASON | RESET_REASON_SYSTEM_FAILURE
        ) {
            return SbiRet::invalid_param();
        }
        match reset_type {
            RESET_TYPE_SHUTDOWN => match reset_reason {
                RESET_REASON_NO_REASON => self.reset_dev.lock().pass(),
                RESET_REASON_SYSTEM_FAILURE => self.reset_dev.lock().fail(u16::MAX),
                value => self.reset_dev.lock().fail(value as _),
            },
            RESET_TYPE_COLD_REBOOT => self.reset_dev.lock().reset(false),
            RESET_TYPE_WARM_REBOOT => {
                let reset_dev = self.reset_dev.lock();
                if !reset_dev.supports_warm_reset() {
                    return SbiRet::not_supported();
                }
                reset_dev.reset(true)
            }

            _ => SbiRet::invalid_param(),
        }
    }
}

#[allow(unused)]
pub fn fail() -> ! {
    match crate::sbi::reset() {
        Some(reset) => reset.fail(),
        None => {
            trace!("test fail, begin dead loop");
            loop {
                core::hint::spin_loop()
            }
        }
    }
}

/// Initializes the SBI reset extension from the discovered board info.
pub(crate) fn init(
    board: &BoardInfo,
    mailbox: Option<&'static crate::rpmi::RpmiMailbox>,
) -> Option<SbiReset> {
    if let Some(base) = board.reset {
        Some(SbiReset::new(Mutex::new(Box::new(
            SifiveTestDeviceWrap::new(base),
        ))))
    } else if let Some((i2c_base, pmic_addr)) = board.pmic_reset {
        Some(SbiReset::new(Mutex::new(Box::new(P1PmicResetWrap::new(
            i2c_base, pmic_addr,
        )))))
    } else if board.rpmi_reset {
        mailbox.and_then(|mailbox| {
            mailbox
                .probe_service_group(crate::rpmi::servicegroup::SYSTEM_RESET)
                .filter(|version| version >> 16 == 1)
                .map(|_| SbiReset::new(Mutex::new(Box::new(RpmiResetWrap::new(mailbox)))))
        })
    } else {
        None
    }
}
