//! Supervisor counter access and overflow state.

use crate::standard::isa::values::counter::CounterSet;

define_csr! {
    /// Supervisor counter-enable register (`scounteren`).
    ///
    /// # Access requirements
    ///
    /// A U-mode counter read requires the corresponding bit in both
    /// `mcounteren` and `scounteren`. Setting this bit does not start the counter.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/supervisor.html#11-1-1-5-counter-enable-scounteren-register>.
    Scounteren { 0x106, r: CounterSet, w: CounterSet };

    /// Supervisor count overflow register (`scountovf`, `Sscofpmf`, read-only).
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/sscofpmf.html>.
    Scountovf { 0xda0, r: usize };
}
