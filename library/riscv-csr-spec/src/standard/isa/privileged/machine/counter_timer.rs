//! Machine cycle, retired-instruction, and programmable counters.

define_csr! {
    /// Machine cycle counter (`mcycle` / RV32 `mcycleh`).
    ///
    /// # Access requirements
    ///
    /// Coordinate writes with all counter users, including sibling harts if shared.
    Mcycle { (0xb00, 0xb80), counter64, r: u64, w: u64 };

    /// Machine retired-instruction counter (`minstret` / RV32 `minstreth`).
    ///
    /// # Access requirements
    ///
    /// Own retired-instruction accounting and coordinate updates on this hart.
    Minstret { (0xb02, 0xb82), counter64, r: u64, w: u64 };
}

define_indexed_csr! {
    /// Machine programmable counter (`mhpmcounter3..=31`, RV32 high halves paired).
    ///
    /// # Access requirements
    ///
    /// Coordinate writes with counter users. Stop the counter and exclude readers
    /// when an exact, non-atomic RV32 update is required.
    Mhpmcounter<INDEX> {
        index: 3..=31, base: 0xb00, high: 0xb80,
        width: crate::Width::Counter64 { high: Self::HIGH_NUMBER },
        r: u64, w: u64
    }
}
