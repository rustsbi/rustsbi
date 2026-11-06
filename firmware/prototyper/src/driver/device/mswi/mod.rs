//! Machine software-interrupt register operations shared by MSWI layouts.
//!
//! # References
//!
//! - Specification: [RISC-V ACLINT 1.0](https://github.com/riscvarchive/riscv-aclint/blob/4e570bfd3201f2c09e5afd290b5091526b0f099a/riscv-aclint.adoc) —
//!   MSIP register layout and pending semantics.
//! - Reference implementation:
//!   [OpenSBI MSWI FDT driver](https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/lib/utils/ipi/fdt_ipi_mswi.c).

mod description;
mod device;

pub(crate) use description::Desc;
