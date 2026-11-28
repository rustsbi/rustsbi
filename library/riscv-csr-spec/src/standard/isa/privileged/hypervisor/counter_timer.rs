//! Virtualized time offset.

define_csr! {
    /// Unified 64-bit time delta for VS/VU-mode `time` (`htimedelta` / RV32 `htimedeltah`).
    ///
    /// # Access requirements
    ///
    /// Coordinate guest clock virtual offsets with vCPU scheduling; RV32 updates
    /// low and high words non-atomically.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#14-1-2-7-hypervisor-time-delta-htimedelta-register>.
    Htimedelta { (0x605, 0x615), r: u64, w: u64 };
}
