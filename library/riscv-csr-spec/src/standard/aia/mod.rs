//! [Advanced Interrupt Architecture v1.0](https://docs.riscv.org/reference/aia/v1.0/CSRs.html) register definitions.
//!
//! The `*h` CSRs expose upper halves only on RV32. Implemented standard local
//! interrupts support atomic pending-bit updates at [M/S level](https://docs.riscv.org/reference/aia/v1.0/MSLevel.html)
//! and [VS level](https://docs.riscv.org/reference/aia/v1.0/VSLevel.html).
//! On RV32 the RAS event bits are in `miph`, `siph` and `vsiph`. On RV64
//! the same bits belong to the full-width pending registers.
//!
//! For `*topei`, a read reports the highest-priority pending and enabled interrupt
//! that passes the file's threshold. An [IMSIC claim](https://docs.riscv.org/reference/aia/v1.0/IMSIC.html#2-1-9-top-external-interrupt-csrs-mtopei-stopei-vstopei)
//! reads and writes the CSR in one
//! instruction, clearing the pending bit of the returned identity. A zero result
//! means no bit was cleared.

pub mod hypervisor;
pub mod imsic;
pub mod interrupt;
pub mod machine;
pub mod supervisor;
pub mod virtual_supervisor;

pub use imsic::{Eidelivery, Eithreshold, Iid, ImsicRegister, TopExternalInterrupt};
pub use interrupt::{MajorIid, TopInterrupt};
