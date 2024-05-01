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

// TODO unconstrained lifetime
pub fn try_read_dynamic(paddr: usize) -> Result<DynamicInfo, ()> {
    // check pointer before dereference
    if !DYNAMIC_INFO_VALID_ADDRESSES.contains(&paddr)
        || !DYNAMIC_INFO_VALID_ADDRESSES.contains(&(paddr + size_of::<DynamicInfo>()))
    {
        return Err(());
    }
    let ans = unsafe { *(paddr as *const DynamicInfo) };
    Ok(ans)
}

pub fn next_mode_mpp(info: &DynamicInfo) -> Result<mstatus::MPP, ()> {
    match info.next_mode {
        0 => Ok(mstatus::MPP::User),
        1 => Ok(mstatus::MPP::Supervisor),
        3 => Ok(mstatus::MPP::Machine),
        _ => Err(()),
    }
}

pub fn check_next_addr(info: &DynamicInfo) -> Result<usize, ()> {
    if NEXT_ADDR_VALID_ADDRESSES.contains(&info.next_addr) {
        Ok(info.next_addr)
    } else {
        Err(())
    }
}
