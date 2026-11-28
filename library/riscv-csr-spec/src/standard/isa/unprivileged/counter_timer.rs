//! Unprivileged counter shadows.

define_csr! {
    /// Architectural cycle counter (`cycle` / RV32 `cycleh`).
    Cycle { (0xc00, 0xc80), counter64, r: u64 };

    /// Architectural wall-clock real-time counter (`time` / RV32 `timeh`).
    ///
    /// Shadows `mtime`. Changes to `mtime` eventually appear in `time`.
    /// A virtualized read includes `htimedelta` modulo 2^64.
    Time { (0xc01, 0xc81), counter64, r: u64 };

    /// Architectural retired-instruction counter (`instret` / RV32 `instreth`).
    Instret { (0xc02, 0xc82), counter64, r: u64 };
}

define_indexed_csr! {
    /// Unprivileged programmable counter shadow (`hpmcounter3..=31`, `Zihpm`).
    Hpmcounter<INDEX> {
        index: 3..=31, base: 0xc00, high: 0xc80,
        width: crate::Width::Counter64 { high: Self::HIGH_NUMBER }, r: u64
    }
}
