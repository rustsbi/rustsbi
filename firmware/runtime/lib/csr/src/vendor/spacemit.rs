//! SpacemiT implementation-specific CSR definitions.

/// K1 machine registers.
///
/// This subset follows the pinned [OpenSBI K1 header] and its [hart setup].
/// The reference does not specify reserved-field semantics or establish
/// compatibility with other SpacemiT processors.
///
/// [OpenSBI K1 header]: https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/platform/generic/include/spacemit/k1.h
/// [hart setup]: https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/platform/generic/spacemit/k1.c
pub mod k1 {
    use crate::{macros::csr, machine::FeatureState};

    csr! {
        /// K1 machine setup (`msetup`).
        MachineSetup { 0x7c0, r,
            /// # Safety
            ///
            /// Execute in M-mode on K1. Coordinate cache, prediction and ECC
            /// transitions with the current memory and coherency configuration.
            /// Preserve undocumented fields.
            unsafe w
        }
            DE[0] { rw: FeatureState }
            IE[1] { rw: FeatureState }
            BPE[4] { rw: FeatureState }
            PFE[5] { rw: FeatureState }
            MME[6] { rw: FeatureState }
            ECCE[16] { rw: FeatureState }
        /// K1 cluster L2 setup (`ml2setup`), accessed by the reset-entry sequence.
        L2Setup { 0x7f0 }
    }

    impl L2Setup {
        /// Selects the hart's index within its four-hart cluster.
        pub const CLUSTER_HART_MASK: usize = 0b11;
        /// Enables cluster hart zero; shift by the local index for other harts.
        pub const FIRST_HART_ENABLE: usize = 1;
    }
}
