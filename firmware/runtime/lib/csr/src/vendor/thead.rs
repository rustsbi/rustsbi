//! T-Head vendor CSRs.

/// T-Head machine-level CSRs.
///
/// Cache and coherency CSR identities follow [OpenSBI's T-Head header], and
/// the cache-operation encoding follows [OpenSBI's reset restore]. Detailed
/// bit layouts vary across T-Head implementations, so full snapshots preserve
/// implementation-defined bits without assuming one core's field meanings.
///
/// [OpenSBI's T-Head header]: https://github.com/YuzukiHD/opensbi/blob/c1ea219a901ff309e88e99c4b4e66ef9a55548de/platform/generic/include/thead/c9xx_encoding.h
/// [OpenSBI's reset restore]: https://github.com/riscv-software-src/opensbi/blob/3593a5facc4c6938b90429a6973ba9ee21fc5899/platform/generic/allwinner/sun20i-d1.c
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

    csr! {
        /// Machine cache configuration (`mhcr`).
        CacheControl { 0x7c1, r,
            /// # Safety
            ///
            /// Execute in M-mode on a T-Head hart with a valid saved
            /// configuration. Coordinate cache transitions and preserve
            /// implementation-defined bits.
            unsafe w
        }
        /// Machine cache configuration register two (`mccr2`).
        L2Control { 0x7c3, r,
            /// # Safety
            ///
            /// Restore a snapshot valid for this T-Head implementation with
            /// cache and coherency state already prepared. Coordinate
            /// conflicting configuration.
            unsafe w
        }
        /// Machine implicit-operation configuration (`mhint`).
        Hint { 0x7c5, r,
            /// # Safety
            ///
            /// Restore a snapshot valid for this T-Head implementation in
            /// M-mode and coordinate other users of the current hart's
            /// configuration.
            unsafe w
        }
        /// Machine cache-operation command port (`mcor`).
        CacheOperation { 0x7c2 }
        /// Machine snoop-control register (`msmpr`).
        SnoopControl { 0x7f3 }
    }

    impl CacheControl {
        /// Sets the supplied `mhcr` bits with CSRRS, retaining every other bit.
        ///
        /// # Safety
        ///
        /// Execute in M-mode on a T-Head hart. Permit the selected cache
        /// transitions and coordinate other users of this hart's cache
        /// configuration.
        #[inline]
        pub unsafe fn set_bits(mask: usize) {
            #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
            // SAFETY: The caller establishes access and cache-transition validity.
            unsafe {
                core::arch::asm!(
                    "csrs {csr}, {mask}",
                    csr = const Self::NUMBER,
                    mask = in(reg) mask,
                    options(nomem, nostack),
                );
            }
            #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
            {
                let _ = mask;
                unimplemented!("T-Head CSR access requires a RISC-V target");
            }
        }
    }

    impl CacheOperation {
        /// Invalidates caches and branch-prediction state during reset restore.
        pub const INVALIDATE_ALL: usize = 0x70013;
    }

    impl SnoopControl {
        /// Enables multi-core cache snoop coherency.
        pub const ENABLE: usize = 1;
    }
}
