//! Supervisor timer compare.
use crate::{macros::csr, values::counter::CounterValue};

csr! {
    /// Supervisor timer deadline (`stimecmp`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/sstc.html>
    TimerCompare { 0x14d, r: CounterValue, w: CounterValue }
    /// Upper supervisor timer deadline bits on RV32 (`stimecmph`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/sstc.html>
    #[cfg(target_pointer_width = "32")]
    TimerCompareHigh { 0x15d, r: CounterValue, w: CounterValue }
}
