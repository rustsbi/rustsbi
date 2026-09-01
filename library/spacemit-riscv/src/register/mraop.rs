//! Machine RAM-array operation register.
//!
//! # Platform support
//!
//! This register is supported on SpacemiT X60, X100, and A100 cores. K1
//! firmware names and accesses it; K3 firmware accesses the same address from
//! cache-maintenance paths shared by X100 and A100 without naming the CSR.
//!
//! The register name comes from K1 firmware; K3 firmware uses the same address
//! without naming it. Public vendor K1 and K3 firmware describes command 2 as
//! invalidating all data cache and command 3 as cleaning and invalidating all
//! data cache. Upstream K1 firmware instead labels command 3 as
//! instruction-cache invalidation. The cache target is therefore not
//! established by a public hardware specification, and the two command bits
//! are intentionally not given separate meanings.
//!
//! [K1 definition](https://github.com/riscv-software-src/opensbi/blob/1f84ec2ac22eaa866d7750f5d7941eb8711fadfc/platform/generic/include/spacemit/k1.h#L4-L24)
//! [K3 OpenSBI source](https://github.com/spacemit-com/opensbi/blob/8bd2cbdf9856dbc1a990d36e26bf47411f356c42/include/sbi_utils/cache/cache.h#L115-L137)

riscv::csr_field_enum! {
    /// Cache-maintenance command.
    ///
    /// The command encodings are inferred from public SpacemiT firmware.
    Operation {
        default: Invalidate,
        /// Invalidate all cache entries.
        ///
        /// Vendor firmware identifies the data cache, while upstream K1
        /// firmware gives the cache target a conflicting description.
        Invalidate = 2,
        /// Clean and invalidate all cache entries.
        ///
        /// Vendor firmware identifies the data cache, while upstream K1
        /// firmware gives the cache target a conflicting description.
        CleanInvalidate = 3,
    },
}

riscv::write_only_csr! {
    /// Machine RAM-array operation command register.
    Mraop: 0x7c2,
    mask: 0x3,
}

riscv::write_only_csr_field! {
    Mraop,
    /// Sets the cache-maintenance operation encoding in bits 1:0.
    ///
    /// The encoding is inferred from public SpacemiT firmware.
    set_operation,
    Operation: [0:1],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mraop_operations() {
        assert_eq!(Operation::Invalidate.into_usize(), 2);
        assert_eq!(Operation::CleanInvalidate.into_usize(), 3);

        let mut value = Mraop::from_bits(0);
        value.set_operation(Operation::Invalidate);
        assert_eq!(value.bits(), 2);

        value.set_operation(Operation::CleanInvalidate);
        assert_eq!(value.bits(), 3);
        assert_eq!(Mraop::from_bits(usize::MAX).bits(), 3);
    }
}
