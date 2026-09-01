//! Feature control register.
//!
//! # Platform support
//!
//! This register is supported on SpacemiT X60 cores. Vendor K1 OpenSBI sets
//! all three fields exposed by this module before leaving M-mode.
//!
//! The register address and field meanings are inferred from public K1 vendor
//! firmware rather than a public SpacemiT hardware specification.
//!
//! [K1 OpenSBI source](https://github.com/spacemit-com/opensbi/blob/fc02b891b17b8bdc1273a39f80aa374cd99ba9a2/lib/sbi/sbi_hart.c#L821-L828)

riscv::read_write_csr! {
    /// K1 feature control register.
    Featurectl: 0xbf9,
    mask: 0x0080_0280,
}

riscv::read_write_csr_field! {
    Featurectl,
    /// Invalidate a clean cache line on write eviction.
    ///
    /// This meaning is inferred from public K1 firmware.
    clean_cacheline_invalidate: 7,
}

riscv::read_write_csr_field! {
    Featurectl,
    /// Fence-operation improvement enable.
    ///
    /// This meaning is inferred from public K1 firmware.
    fence_improvement: 9,
}

riscv::read_write_csr_field! {
    Featurectl,
    /// Vector load/store dual-issue disable.
    ///
    /// This meaning is inferred from public K1 firmware.
    vector_ls_dual_issue_disable: 23,
}

riscv::set!(0xbf9);
riscv::clear!(0xbf9);

riscv::set_clear_csr!(
    /// Invalidate a clean cache line on write eviction.
    ///
    /// This meaning is inferred from public K1 firmware.
    , set_clean_cacheline_invalidate, clear_clean_cacheline_invalidate, 1 << 7);
riscv::set_clear_csr!(
    /// Fence-operation improvement enable.
    ///
    /// This meaning is inferred from public K1 firmware.
    , set_fence_improvement, clear_fence_improvement, 1 << 9);
riscv::set_clear_csr!(
    /// Vector load/store dual-issue disable.
    ///
    /// This meaning is inferred from public K1 firmware.
    , set_vector_ls_dual_issue_disable, clear_vector_ls_dual_issue_disable, 1 << 23);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn featurectl_fields() {
        let mut value = Featurectl::from_bits(0);
        assert_eq!(Featurectl::BITMASK, 0x0080_0280);

        value.set_clean_cacheline_invalidate(true);
        value.set_fence_improvement(true);
        value.set_vector_ls_dual_issue_disable(true);

        assert!(value.clean_cacheline_invalidate());
        assert!(value.fence_improvement());
        assert!(value.vector_ls_dual_issue_disable());
        assert_eq!(value.bits(), 0x0080_0280);
    }
}
