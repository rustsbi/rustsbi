//! Machine timer register operations shared by MTIMER layouts.
//!
//! # References
//!
//! - Specification: [RISC-V ACLINT 1.0](https://github.com/riscvarchive/riscv-aclint/blob/4e570bfd3201f2c09e5afd290b5091526b0f099a/riscv-aclint.adoc) —
//!   MTIME and per-HART MTIMECMP register maps.
//! - Specification: [RISC-V Privileged Architecture](https://docs.riscv.org/reference/isa/priv/machine.html) —
//!   machine timer interrupts and 64-bit MTIMECMP compare registers.
//! - Reference implementation:
//!   [OpenSBI MTIMER accessors](https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/lib/utils/timer/aclint_mtimer.c).

mod description;
mod device;

pub(crate) use description::{CompareAccess, Desc};
