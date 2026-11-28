//! Counter identities and enable sets shared by `*counteren` registers.
//!
//! Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#mcounteren>.

/// Fixed architectural counter indices in `*counteren`.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Counter {
    /// Cycle counter (`cycle` / `mcycle`, bit 0).
    Cycle = 0,
    /// Timer (`time`, bit 1).
    Time = 1,
    /// Retired instructions (`instret` / `minstret`, bit 2).
    InstructionsRetired = 2,
}

csr_value! {
    /// 32-bit architectural counter-enable set (`mcounteren`, `scounteren`, `hcounteren`).
    pub struct CounterSet(u32);
}

impl CounterSet {
    /// Returns a mask with all 32 counter-enable bits set.
    #[inline(always)]
    pub const fn all() -> Self {
        Self(u32::MAX)
    }

    /// Reports whether this mask contains `counter`.
    #[inline(always)]
    pub const fn contains(self, counter: Counter) -> bool {
        (self.0 & (1u32 << counter as u8)) != 0
    }
}
