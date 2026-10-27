//! CLINT register-layout facts shared by the timer and IPI class drivers.
//!
//! The timer class has one driver per layout: SiFive exposes `mtime`, while
//! T-Head has no memory-mapped time register and splits `mtimecmp` into two
//! 32-bit words. The IPI class has one driver because both layouts share the
//! same MSIP window.

/// Compatibles handled by the SiFive-layout timer driver.
pub(crate) const SIFIVE_COMPATIBLES: [&str; 3] =
    ["riscv,clint0", "starfive,jh7110-clint", "sifive,clint0"];

/// Compatibles handled by the T-Head-layout timer driver.
pub(crate) const THEAD_COMPATIBLES: [&str; 1] = ["thead,c900-clint"];

/// Device-tree `compatible` values understood by the CLINT IPI driver.
pub(crate) const COMPATIBLES: [&str; 4] = [
    SIFIVE_COMPATIBLES[0],
    SIFIVE_COMPATIBLES[1],
    SIFIVE_COMPATIBLES[2],
    THEAD_COMPATIBLES[0],
];

/// Offset of the first `mtimecmp` word in both supported layouts.
pub(crate) const MTIMECMP_OFFSET: usize = 0x4000;
