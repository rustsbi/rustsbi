//! Frequently used first boot stage dynamic information on RISC-V.

use core::ops::Range;

use super::BootInfo;
use crate::fail;

use riscv::register::mstatus;

/// Gets boot information from nonstandard_a2 parameter.
///
/// Returns BootInfo containing next stage address and privilege mode.
pub fn get_boot_info(nonstandard_a2: usize) -> BootInfo {
    let dynamic_info = read_paddr(nonstandard_a2).unwrap_or_else(fail::no_dynamic_info_available);
    let (mpp, next_addr) = mpp_next_addr(&dynamic_info).unwrap_or_else(fail::invalid_dynamic_data);
    BootInfo {
        next_address: next_addr,
        mpp,
    }
}

/// M-mode firmware dynamic information.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct DynamicInfo {
    /// Dynamic information magic value.
    pub magic: usize,
    /// Version of dynamic information.
    pub version: usize,
    /// Address of the next boot-loading stage.
    pub next_addr: usize,
    /// RISC-V privilege mode of the next boot-loading stage.
    pub next_mode: usize,
    /// M-mode firmware options; its definition varies between SBI implementations.
    pub options: usize,
    /// Boot hart ID of current environment.
    pub boot_hart: usize,
}

// Definition of `boot_hart` can be found at:
// https://github.com/riscv-software-src/opensbi/blob/019a8e69a1dc0c0f011fabd0372e1ba80e40dd7c/include/sbi/fw_dynamic.h#L75

const DYNAMIC_INFO_INVALID_ADDRESSES: usize = 0x00000000;
pub(crate) const MAGIC: usize = 0x4942534f;
const SUPPORTED_VERSION: Range<usize> = 0..3;

/// Error type for dynamic info read failures.
pub struct DynamicReadError {
    pub bad_paddr: Option<usize>,
    pub bad_magic: Option<usize>,
    pub bad_version: Option<usize>,
}

// TODO: unconstrained lifetime
/// Reads dynamic info from physical address.
///
/// Returns Result containing DynamicInfo or error details.
pub fn read_paddr(paddr: usize) -> Result<DynamicInfo, DynamicReadError> {
    let mut error = DynamicReadError {
        bad_paddr: None,
        bad_magic: None,
        bad_version: None,
    };
    // check pointer before dereference.
    if DYNAMIC_INFO_INVALID_ADDRESSES == paddr {
        error.bad_paddr = Some(paddr);
        return Err(error);
    }
    let ans = unsafe { *(paddr as *const DynamicInfo) };

    // Validate magic number and version.
    if ans.magic != MAGIC {
        error.bad_magic = Some(ans.magic);
    }
    if !SUPPORTED_VERSION.contains(&ans.version) {
        error.bad_version = Some(ans.version);
    }
    if error.bad_magic.is_some() || error.bad_version.is_some() {
        return Err(error);
    }
    Ok(ans)
}

/// Error type for dynamic info validation failures.
pub struct DynamicError<'a> {
    pub invalid_mpp: bool,
    pub invalid_next_addr: bool,
    pub bad_info: &'a DynamicInfo,
}

/// Validates and extracts privilege mode and next address from dynamic info.
///
/// Returns Result containing tuple of (MPP, next_addr) or error details.
pub fn mpp_next_addr(info: &DynamicInfo) -> Result<(mstatus::MPP, usize), DynamicError<'_>> {
    let mut error = DynamicError {
        invalid_mpp: false,
        invalid_next_addr: false,
        bad_info: info,
    };

    // fail safe, errors will be aggregated after whole checking process.
    let next_addr_valid = crate::cfg::DYNAMIC_NEXT_ADDR_RANGE
        .iter()
        .any(|r| info.next_addr >= r.start as usize && info.next_addr < r.end as usize);
    let mpp_valid = matches!(info.next_mode, 0 | 1 | 3);

    if !next_addr_valid {
        error.invalid_next_addr = true;
    }
    if !mpp_valid {
        error.invalid_mpp = true;
    }

    if !next_addr_valid || !mpp_valid {
        return Err(error);
    }

    let mpp = match info.next_mode {
        3 => mstatus::MPP::Machine,
        1 => mstatus::MPP::Supervisor,
        // pattern `_` avoids `unreachable!`` which introduces panic handler.
        // pattern 0 and _
        _ => mstatus::MPP::User,
    };

    Ok((mpp, info.next_addr))
}
