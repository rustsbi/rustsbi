//! Machine-mode CSR definitions.
use crate::macros::csr;

csr! {
    /// [RISC-V Privileged Architecture: mhartid](https://docs.riscv.org/reference/isa/priv/machine.html#mhartid).
    HartId { 0xf14, RO, value: usize }

    /// [RISC-V Privileged Architecture: mcounteren](https://docs.riscv.org/reference/isa/priv/machine.html#mcounteren).
    CounterEnable { 0x306, RW, value: u32 }
}
