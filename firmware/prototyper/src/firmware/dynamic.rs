//! Dynamic firmware handoff information on RISC-V.
//!
//! # References
//!
//! - Compatibility reference: [OpenSBI `fw_dynamic` interface](https://github.com/riscv-software-src/opensbi/blob/019a8e69a1dc0c0f011fabd0372e1ba80e40dd7c/include/sbi/fw_dynamic.h) —
//!   `DynamicInfo` layout, magic value, versions, and boot-hart encoding.

use core::ops::Range;

use crate::fail;

use riscv::register::mstatus;

/// Derives the next-stage address and privilege mode from the `a2`
/// `DynamicInfo`; prints and stops on invalid input.
pub(crate) fn decode_next_stage(dynamic_info_address: usize) -> (mstatus::MPP, usize) {
    let dynamic_info =
        read_dynamic_info(dynamic_info_address).unwrap_or_else(fail::no_dynamic_info_available);
    validate_next_stage(&dynamic_info).unwrap_or_else(fail::invalid_dynamic_data)
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

const NULL_DYNAMIC_INFO_ADDRESS: usize = 0;
pub(crate) const MAGIC: usize = 0x4942534f;
const SUPPORTED_VERSION: Range<usize> = 0..3;

/// Error type for dynamic info read failures.
pub struct ReadError {
    pub invalid_address: Option<usize>,
    pub invalid_magic: Option<usize>,
    pub unsupported_version: Option<usize>,
}

// TODO: unconstrained lifetime
/// Reads dynamic info from physical address.
///
/// Returns Result containing DynamicInfo or error details.
pub fn read_dynamic_info(address: usize) -> Result<DynamicInfo, ReadError> {
    let mut error = ReadError {
        invalid_address: None,
        invalid_magic: None,
        unsupported_version: None,
    };
    // check pointer before dereference.
    if address == NULL_DYNAMIC_INFO_ADDRESS {
        error.invalid_address = Some(address);
        return Err(error);
    }
    let dynamic_info = unsafe { *(address as *const DynamicInfo) };

    // Validate magic number and version.
    if dynamic_info.magic != MAGIC {
        error.invalid_magic = Some(dynamic_info.magic);
    }
    if !SUPPORTED_VERSION.contains(&dynamic_info.version) {
        error.unsupported_version = Some(dynamic_info.version);
    }
    if error.invalid_magic.is_some() || error.unsupported_version.is_some() {
        return Err(error);
    }
    Ok(dynamic_info)
}

/// Error type for dynamic info validation failures.
pub struct ValidationError<'a> {
    pub invalid_next_mode: bool,
    pub invalid_next_address: bool,
    pub dynamic_info: &'a DynamicInfo,
}

/// Validates and extracts privilege mode and next address from dynamic info.
///
/// Returns Result containing tuple of (MPP, next_addr) or error details.
pub fn validate_next_stage(
    dynamic_info: &DynamicInfo,
) -> Result<(mstatus::MPP, usize), ValidationError<'_>> {
    let mut error = ValidationError {
        invalid_next_mode: false,
        invalid_next_address: false,
        dynamic_info,
    };

    // fail safe, errors will be aggregated after whole checking process.
    let is_next_address_valid = crate::cfg::DYNAMIC_NEXT_ADDR_RANGE.iter().any(|range| {
        dynamic_info.next_addr >= range.start as usize
            && dynamic_info.next_addr < range.end as usize
    });
    let is_next_mode_valid = matches!(dynamic_info.next_mode, 0 | 1 | 3);

    if !is_next_address_valid {
        error.invalid_next_address = true;
    }
    if !is_next_mode_valid {
        error.invalid_next_mode = true;
    }

    if !is_next_address_valid || !is_next_mode_valid {
        return Err(error);
    }

    let next_mode = match dynamic_info.next_mode {
        3 => mstatus::MPP::Machine,
        1 => mstatus::MPP::Supervisor,
        // pattern `_` avoids `unreachable!`` which introduces panic handler.
        // pattern 0 and _
        _ => mstatus::MPP::User,
    };

    Ok((next_mode, dynamic_info.next_addr))
}
