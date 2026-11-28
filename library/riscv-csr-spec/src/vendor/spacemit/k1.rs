//! SpacemiT K1 (X60) custom CSRs documented by the cited sources.
//!
//! Hardware accesses require M-mode on K1.
//!
//! Register definitions and reset-entry use:
//! [OpenSBI `spacemit/k1.h`](https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/platform/generic/include/spacemit/k1.h)
//! and [`spacemit/k1.c`](https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/platform/generic/spacemit/k1.c).

define_csr! {
    /// Cache, predictor, and ECC controls.
    ///
    /// # Access requirements
    ///
    /// Coordinate cache, predictor, and ECC transitions and preserve undocumented bits.
    Msetup { 0x7c0, r, w } {
        /// D-cache enable.
        DE[0] { rw: bool }
        /// I-cache enable.
        IE[1] { rw: bool }
        /// Branch prediction enable.
        BPE[4] { rw: bool }
        /// Hardware prefetch enable.
        PFE[5] { rw: bool }
        /// Misaligned memory access enable.
        MME[6] { rw: bool }
        /// ECC enable.
        ECCE[16] { rw: bool }
    }

    /// Cluster L2 setup used during reset entry.
    Ml2setup { 0x7f0 };
}

impl Ml2setup {
    /// Mask for extracting the hart index within a four-hart cluster.
    pub const CLUSTER_HART_MASK: usize = 0b11;
    /// Bit enabling cluster hart 0; shift left by local cluster hart index for harts `1..=3`.
    pub const FIRST_HART_ENABLE: usize = 1;
}
