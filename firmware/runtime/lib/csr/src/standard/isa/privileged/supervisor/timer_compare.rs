//! Supervisor timer compare.
use crate::{macros::csr, values::counter::CounterValue};

csr! {
    /// Upper half of supervisor timer compare on RV32 (`stimecmph`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/sstc.html>.
    TimerCompare { 0x14d, r: CounterValue, w: CounterValue }
    /// Upper half of supervisor timer compare on RV32 (`stimecmph`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/sstc.html>.
    #[cfg(target_pointer_width = "32")]
    TimerCompareHigh { 0x15d, r: CounterValue, w: CounterValue }
}
