//! Zicntr counters and timers.
use crate::macros::csr;
use crate::values::counter::CounterValue;

csr! {
    /// Read-only real-time counter shadow (`time`).
    ///
    /// <https://docs.riscv.org/reference/isa/unpriv/counters.html>
    Time { 0xc01, r: CounterValue }
    /// Upper real-time counter bits on RV32 (`timeh`).
    ///
    /// <https://docs.riscv.org/reference/isa/unpriv/counters.html>
    #[cfg(target_pointer_width = "32")]
    TimeHigh { 0xc81, r: CounterValue }
}
