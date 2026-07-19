//! Non-RISC-V backend: privileged accesses are unavailable.

use super::super::ExpectedResult;

pub(crate) unsafe fn probe_csr<const CSR: u16>() -> ExpectedResult {
    let _ = CSR;
    ExpectedResult::Unavailable
}

pub(crate) unsafe fn swap_csr<const CSR: u16>(value: usize) -> ExpectedResult {
    let _ = (CSR, value);
    ExpectedResult::Unavailable
}

pub(crate) unsafe fn load_byte(address: usize) -> ExpectedResult {
    let _ = address;
    ExpectedResult::Unavailable
}

pub(crate) unsafe fn store_byte(address: usize, byte: u8) -> ExpectedResult {
    let _ = (address, byte);
    ExpectedResult::Unavailable
}
