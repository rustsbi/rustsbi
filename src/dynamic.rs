//! Frequently used first boot stage dynamic information on RISC-V.

use core::{mem::size_of, ops::Range};

use riscv::register::mstatus;

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
}

const DYNAMIC_INFO_VALID_ADDRESSES: Range<usize> = 0x1000..0xf000;
const NEXT_ADDR_VALID_ADDRESSES: Range<usize> = 0x80000000..0x90000000;

pub struct DynamicReadError {
    pub bad_paddr: usize,
}

// TODO unconstrained lifetime
pub fn read_paddr(paddr: usize) -> Result<DynamicInfo, DynamicReadError> {
    // check pointer before dereference
    if !DYNAMIC_INFO_VALID_ADDRESSES.contains(&paddr)
        || !DYNAMIC_INFO_VALID_ADDRESSES.contains(&(paddr + size_of::<DynamicInfo>()))
    {
        return Err(DynamicReadError { bad_paddr: paddr });
    }
    let ans = unsafe { *(paddr as *const DynamicInfo) };
    Ok(ans)
}

pub struct DynamicError<'a> {
    pub invalid_mpp: bool,
    pub invalid_next_addr: bool,
    pub bad_info: &'a DynamicInfo,
}

pub fn mpp_next_addr(info: &DynamicInfo) -> Result<(mstatus::MPP, usize), DynamicError> {
    let mut error = DynamicError {
        invalid_mpp: false,
        invalid_next_addr: false,
        bad_info: info,
    };

    // fail safe, errors will be aggregated after whole checking process.
    let next_addr_valid = NEXT_ADDR_VALID_ADDRESSES.contains(&info.next_addr);
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
        0 | _ => mstatus::MPP::User,
    };

    Ok((mpp, info.next_addr))
}
