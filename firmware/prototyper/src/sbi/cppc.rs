use rustsbi::SbiRet;
use spin::Mutex;

use crate::rpmi::servicegroup;
use crate::rpmi::{CppcProbeReq, CppcReadReq, CppcWriteReq, RpmiMailbox};
use ::rpmi::cppc;
use ::rpmi::message::Status as RpmiError;

use crate::riscv::current_hartid;

const LAST_ACPI_REG_ID: u32 = 0x14;
const TRANSITION_LATENCY_REG_ID: u32 = 0x8000_0000;

fn valid_reg_id(reg_id: u32) -> bool {
    reg_id <= LAST_ACPI_REG_ID || reg_id == TRANSITION_LATENCY_REG_ID
}

fn rpmi_access_error_to_sbi(err: RpmiError) -> SbiRet {
    match err {
        RpmiError::NotSupported => SbiRet::not_supported(),
        RpmiError::Denied => SbiRet::denied(),
        _ => SbiRet::failed(),
    }
}

/// CPPC extension backed by the RPMI CPPC service group.
///
/// Without a mailbox backend, probes report zero-width registers and accesses
/// are not supported.
pub(crate) struct SbiCppc {
    mailbox: Mutex<Option<&'static RpmiMailbox>>,
}

impl SbiCppc {
    /// Create a new CPPC extension without a mailbox backend.
    pub(crate) const fn new() -> Self {
        Self {
            mailbox: Mutex::new(None),
        }
    }

    /// Inject the platform mailbox backend.
    pub(crate) fn set_mailbox(&self, mailbox: &'static RpmiMailbox) {
        *self.mailbox.lock() = Some(mailbox);
    }
}

impl rustsbi::Cppc for SbiCppc {
    fn probe(&self, reg_id: u32) -> SbiRet {
        if !valid_reg_id(reg_id) {
            return SbiRet::invalid_param();
        }
        let mailbox = self.mailbox.lock();
        let Some(mbox) = mailbox.as_ref().copied() else {
            // No backend: register not implemented (width 0).
            return SbiRet::success(0);
        };
        let req = CppcProbeReq {
            hart_id: current_hartid() as u32,
            reg_id,
        }
        .to_bytes();
        let mut resp = [0u8; 8];
        match mbox.normal_request_with_status(servicegroup::CPPC, cppc::PROBE_REG, &req, &mut resp)
        {
            Ok((RpmiError::Success, len)) if len >= resp.len() => {
                let reg_len = u32::from_le_bytes([resp[4], resp[5], resp[6], resp[7]]);
                SbiRet::success(reg_len as usize)
            }
            Ok((RpmiError::NotSupported, _)) => SbiRet::success(0),
            Ok(_) => SbiRet::failed(),
            Err(_) => SbiRet::failed(),
        }
    }

    fn read(&self, reg_id: u32) -> SbiRet {
        if !valid_reg_id(reg_id) {
            return SbiRet::invalid_param();
        }
        let mailbox = self.mailbox.lock();
        let Some(mbox) = mailbox.as_ref().copied() else {
            return SbiRet::not_supported();
        };
        let req = CppcReadReq {
            hart_id: current_hartid() as u32,
            reg_id,
        }
        .to_bytes();
        let mut resp = [0u8; 12];
        match mbox.normal_request_with_status(servicegroup::CPPC, cppc::READ_REG, &req, &mut resp) {
            Ok((RpmiError::Success, len)) if len >= resp.len() => {
                let lo = u32::from_le_bytes([resp[4], resp[5], resp[6], resp[7]]);
                SbiRet::success(lo as usize)
            }
            Ok((err, _)) => rpmi_access_error_to_sbi(err),
            Err(_) => SbiRet::failed(),
        }
    }

    fn read_hi(&self, reg_id: u32) -> SbiRet {
        if !valid_reg_id(reg_id) {
            return SbiRet::invalid_param();
        }
        if cfg!(target_pointer_width = "64") {
            return SbiRet::success(0);
        }
        let mailbox = self.mailbox.lock();
        let Some(mbox) = mailbox.as_ref().copied() else {
            return SbiRet::not_supported();
        };
        let req = CppcReadReq {
            hart_id: current_hartid() as u32,
            reg_id,
        }
        .to_bytes();
        let mut resp = [0u8; 12];
        match mbox.normal_request_with_status(servicegroup::CPPC, cppc::READ_REG, &req, &mut resp) {
            Ok((RpmiError::Success, len)) if len >= resp.len() => {
                let hi = u32::from_le_bytes([resp[8], resp[9], resp[10], resp[11]]);
                SbiRet::success(hi as usize)
            }
            Ok((err, _)) => rpmi_access_error_to_sbi(err),
            Err(_) => SbiRet::failed(),
        }
    }

    fn write(&self, reg_id: u32, val: u64) -> SbiRet {
        if !valid_reg_id(reg_id) {
            return SbiRet::invalid_param();
        }
        let mailbox = self.mailbox.lock();
        let Some(mbox) = mailbox.as_ref().copied() else {
            return SbiRet::not_supported();
        };
        let req = CppcWriteReq {
            hart_id: current_hartid() as u32,
            reg_id,
            data_lo: val as u32,
            data_hi: (val >> 32) as u32,
        }
        .to_bytes();
        let mut resp = [0u8; 4];
        match mbox.normal_request_with_status(servicegroup::CPPC, cppc::WRITE_REG, &req, &mut resp)
        {
            Ok((RpmiError::Success, len)) if len >= resp.len() => SbiRet::success(0),
            Ok((err, _)) => rpmi_access_error_to_sbi(err),
            Err(_) => SbiRet::failed(),
        }
    }
}
