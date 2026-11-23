//! T-Head vendor CSRs.

/// T-Head machine-level CSRs.
pub mod machine {
    use crate::macros::csr;
    use crate::machine::FeatureState;

    csr! {
        /// T-Head extended status register (`mxstatus`).
        ///
        /// Xuantie OpenC910 hardware source code defines this CSR number as
        /// `0x7c0`:
        /// <https://github.com/XUANTIE-RV/openc910/blob/b91c90914c19f114d35c8f6b73408eb241ed847c/C910_RTL_FACTORY/gen_rtl/cp0/rtl/ct_cp0_iui.v#L615>.
        ExtendedStatus { 0x7c0, r }
            /// Machine acceleration execution environment (`MAEE`).
            MAEE[21] { rw: FeatureState }
    }

    impl ExtendedStatus {
        /// Clears MAEE with one CSRRC operation, preserving other fields.
        ///
        /// # Safety
        ///
        /// Execute in M-mode on a T-Head hart with `mxstatus`. Own translation
        /// setup, keep lower-mode translation inactive, and coordinate updates.
        #[inline]
        pub unsafe fn clear_maee() {
            #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
            // SAFETY: The caller owns translation setup and establishes access.
            unsafe {
                core::arch::asm!(
                    "csrc {csr}, {mask}",
                    csr = const Self::NUMBER,
                    mask = in(reg) MAEE::MASK,
                    options(nomem, nostack),
                );
            }
            #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
            unimplemented!("T-Head CSR access requires a RISC-V target");
        }

        /// Restores a complete `mxstatus` snapshot, including implementation bits.
        ///
        /// # Safety
        ///
        /// Use a snapshot valid for this T-Head implementation in M-mode.
        /// Preserve the live execution, translation, cache and interrupt context
        /// and coordinate other users of this hart's configuration.
        #[inline]
        pub unsafe fn restore(snapshot: Self) {
            // SAFETY: The caller establishes the complete snapshot's validity.
            unsafe { crate::value::write::<{ Self::NUMBER }>(snapshot.0) };
        }
    }
}

/// C907 cache and coherency registers used by V861.
///
/// The [V861 register header] supplies CSR identities; its [platform source]
/// supplies opaque startup values. Detailed C907 bit layouts are not specified
/// there; full snapshots retain implementation state without borrowing another
/// core's field meanings.
/// The cache-operation encoding also follows [OpenSBI's reset restore].
///
/// [V861 register header]: https://github.com/YuzukiHD/opensbi/blob/c1ea219a901ff309e88e99c4b4e66ef9a55548de/platform/generic/include/thead/c9xx_encoding.h
/// [platform source]: https://github.com/YuzukiHD/opensbi/blob/c1ea219a901ff309e88e99c4b4e66ef9a55548de/platform/generic/allwinner/sun252i-v861.c
/// [OpenSBI's reset restore]: https://github.com/riscv-software-src/opensbi/blob/3593a5facc4c6938b90429a6973ba9ee21fc5899/platform/generic/allwinner/sun20i-d1.c
pub mod c907 {
    use crate::macros::csr;

    csr! {
        /// Machine cache configuration (`mhcr`).
        CacheControl { 0x7c1, r,
            /// # Safety
            ///
            /// Execute in M-mode on C907 with a valid saved configuration.
            /// Coordinate cache transitions and preserve undocumented bits.
            unsafe w
        }
        /// Machine cache configuration register two (`mccr2`).
        L2Control { 0x7c3, r,
            /// # Safety
            ///
            /// Restore a snapshot valid for this C907 with cache and coherency
            /// state already prepared. Coordinate conflicting configuration.
            unsafe w
        }
        /// Machine implicit-operation configuration (`mhint`).
        Hint { 0x7c5, r,
            /// # Safety
            ///
            /// Restore a snapshot valid for this C907 in M-mode and coordinate
            /// other users of the current hart's configuration.
            unsafe w
        }
        /// Machine cache-operation command port (`mcor`).
        CacheOperation { 0x7c2 }
        /// Machine snoop-control register (`msmpr`).
        SnoopControl { 0x7f3 }
    }

    impl CacheControl {
        /// The V861 firmware's set-bit operand; its individual bits are opaque.
        pub const V861_ENABLE: Self = Self((1 << 24) | (1 << 12));

        /// Sets the supplied `mhcr` bits with CSRRS, retaining every other bit.
        ///
        /// # Safety
        ///
        /// Execute in M-mode on C907. Permit the selected cache transitions and
        /// coordinate other users of this hart's cache configuration.
        #[inline]
        pub unsafe fn set_bits(mask: Self) {
            #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
            // SAFETY: The caller establishes access and cache-transition validity.
            unsafe {
                core::arch::asm!(
                    "csrs {csr}, {mask}",
                    csr = const Self::NUMBER,
                    mask = in(reg) mask.0,
                    options(nomem, nostack),
                );
            }
            #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
            {
                let _ = mask;
                unimplemented!("C907 CSR access requires a RISC-V target");
            }
        }
    }

    impl CacheOperation {
        /// Invalidates caches and branch-prediction state during reset restore.
        pub const INVALIDATE_ALL: usize = 0x70013;
    }

    impl SnoopControl {
        /// The V861 reset path's coherency-enable operand.
        pub const ENABLE: usize = 1;
    }
}
