//! Zicntr counters and timers.
use crate::macros::csr;
use crate::values::counter::CounterValue;

csr! {
    /// Architectural time counter (`time`).
    ///
    /// See <https://docs.riscv.org/reference/isa/unpriv/counters.html>.
    Time { 0xc01, r: CounterValue }
    /// Upper half of the architectural time counter on RV32 (`timeh`).
    ///
    /// See <https://docs.riscv.org/reference/isa/unpriv/counters.html>.
    TimeHigh { 0xc81, r: CounterValue }
}
