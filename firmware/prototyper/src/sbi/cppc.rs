//! Collaborative Processor Performance Control.
//!
//! # References
//!
//! - Specification: [RISC-V SBI CPPC extension](https://docs.riscv.org/reference/sbi/v3.0/ext-cppc.html) —
//!   register discovery and access operations.

use rustsbi::SbiRet;

/// CPPC extension for platforms without a register backend.
pub(crate) struct SbiCppc;

impl SbiCppc {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl rustsbi::Cppc for SbiCppc {
    fn probe(&self, _reg_id: u32) -> SbiRet {
        SbiRet::success(0)
    }

    fn read(&self, _reg_id: u32) -> SbiRet {
        SbiRet::not_supported()
    }

    fn read_hi(&self, _reg_id: u32) -> SbiRet {
        SbiRet::not_supported()
    }

    fn write(&self, _reg_id: u32, _val: u64) -> SbiRet {
        SbiRet::not_supported()
    }
}
