//! Collaborative Processor Performance Control.
//!
//! # References
//!
//! - Specification: [RISC-V SBI CPPC extension](https://docs.riscv.org/reference/sbi/v3.0/ext-cppc.html) —
//!   register discovery and access operations.

#![forbid(unsafe_code)]

use runtime::rustsbi::{Cppc, SbiRet};

// SBI v3.0 defines the ACPI CPPC register range and one transition latency register.
const ACPI_CPPC_MAX_REG_ID: u32 = 0x0000_0014;
const SBI_CPPC_TRANSITION_LATENCY_REG_ID: u32 = 0x8000_0000;

/// CPPC extension for platforms without a register backend.
pub(crate) struct SbiCppc;

impl SbiCppc {
    pub(crate) const fn new() -> Self {
        Self
    }
}

fn is_reserved_reg(reg_id: u32) -> bool {
    reg_id > ACPI_CPPC_MAX_REG_ID && reg_id != SBI_CPPC_TRANSITION_LATENCY_REG_ID
}

impl Cppc for SbiCppc {
    fn probe(&self, reg_id: u32) -> SbiRet {
        if is_reserved_reg(reg_id) {
            return SbiRet::invalid_param();
        }
        SbiRet::success(0)
    }

    fn read(&self, reg_id: u32) -> SbiRet {
        if is_reserved_reg(reg_id) {
            return SbiRet::invalid_param();
        }
        SbiRet::not_supported()
    }

    fn read_hi(&self, reg_id: u32) -> SbiRet {
        if is_reserved_reg(reg_id) {
            return SbiRet::invalid_param();
        }
        SbiRet::not_supported()
    }

    fn write(&self, reg_id: u32, _val: u64) -> SbiRet {
        if is_reserved_reg(reg_id) {
            return SbiRet::invalid_param();
        }
        SbiRet::not_supported()
    }
}
