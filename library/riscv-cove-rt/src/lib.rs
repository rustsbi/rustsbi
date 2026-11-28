//! Host calls for the [RISC-V Confidential VM Extension (CoVE)](https://github.com/riscv-non-isa/riscv-ap-tee/blob/v0.7/src/sbi_cove.adoc).
//!
//! COVH passes each address in one `unsigned long`.
//! If a [`Physical`](sbi_spec::binary::Physical) argument has a nonzero high
//! address word, the wrapper returns `SBI_ERR_INVALID_ADDRESS` before issuing the call.
#![no_std]

mod host;

pub use host::*;
