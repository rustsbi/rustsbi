//! Integer register widths encoded by `misa` and status registers.
//!
//! [`misa.MXL`](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#misa),
//! [`mstatus.SXL/UXL`](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#2-1-1-6-machine-status-mstatus-and-mstatush-registers)
//! and [`hstatus.VSXL`](https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#sec:hstatus)
//! use the same encoding for 32-bit and 64-bit operation.

field_enum! {
    /// Standard integer register width encoding.
    ///
    /// Zero has a field-specific meaning. The encoding three is reserved.
    pub enum Xlen {
        /// Integer registers are 32 bits wide.
        X32 = 1,
        /// Integer registers are 64 bits wide.
        X64 = 2,
    }
}

impl Xlen {
    /// Width of the integer word used to compile the current program.
    pub const NATIVE: Self = match usize::BITS {
        32 => Self::X32,
        64 => Self::X64,
        _ => panic!("unsupported native CSR width"),
    };

    /// Returns the integer register width in bytes.
    pub const fn bytes(self) -> usize {
        match self {
            Self::X32 => 4,
            Self::X64 => 8,
        }
    }
}
