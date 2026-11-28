//! T-Head custom CSRs with source-specific applicability.
//!
//! Hardware accesses require M-mode on an implementation supporting the selected
//! register. Restores through `Mxstatus`, `Mccr2`, and `Mhint` require a valid saved
//! snapshot for that implementation.
//!
//! Shared sources:
//!
//! - [OpenSBI T-Head `c9xx_encoding.h`](https://github.com/YuzukiHD/opensbi/blob/c1ea219a901ff309e88e99c4b4e66ef9a55548de/platform/generic/include/thead/c9xx_encoding.h):
//!   numbers for `Mxstatus`, `Mhcr`, `Mccr2`, `Mhint`, `Mcor`, and `Msmpr`.
//! - [OpenSBI `sun20i-d1` reset restore](https://github.com/riscv-software-src/opensbi/blob/3593a5facc4c6938b90429a6973ba9ee21fc5899/platform/generic/allwinner/sun20i-d1.c):
//!   reset-entry use of `Mxstatus`, `Mccr2`, `Mhint`, `Mcor`, and `Msmpr`.

define_csr! {
    /// Extended execution and translation status.
    ///
    /// # Access requirements
    ///
    /// Preserve live execution and translation context during whole-register
    /// restores. Clear `MAEE` only while owning translation setup with lower-mode
    /// translation inactive.
    ///
    /// [OpenC910 MAEE write bit](https://github.com/XUANTIE-RV/openc910/blob/b91c90914c19f114d35c8f6b73408eb241ed847c/C910_RTL_FACTORY/gen_rtl/cp0/rtl/ct_cp0_regs.v#L2735).
    Mxstatus { 0x7c0, r, w(Self | Clear) } {
        /// Extended page attribute enable (`MAEE`).
        MAEE[21] { rw: bool, bit }
    }

    /// Machine hardware configuration.
    ///
    /// # Access requirements
    ///
    /// Coordinate cache and branch-predictor transitions and preserve
    /// implementation-defined bits.
    ///
    /// Reference (OpenSBI `sun252i-v861.c`):
    /// <https://github.com/YuzukiHD/opensbi/blob/c1ea219a901ff309e88e99c4b4e66ef9a55548de/platform/generic/allwinner/sun252i-v861.c>.
    Mhcr { 0x7c1, r: usize, w(usize | Set) };

    /// Cache configuration register 2.
    ///
    /// # Access requirements
    ///
    /// Prepare cache and coherency setup before restoring this register.
    Mccr2 { 0x7c3, r: usize, w: usize };

    /// Implicit-operation hint configuration.
    ///
    /// # Access requirements
    ///
    /// Coordinate concurrent hart configuration users.
    Mhint { 0x7c5, r: usize, w: usize };

    /// Cache-operation command port.
    Mcor { 0x7c2 };

    /// Snoop control.
    Msmpr { 0x7f3 };
}

impl Mcor {
    /// Command word that invalidates caches and branch-prediction state on reset entry.
    pub const INVALIDATE_ALL: usize = 0x70013;
}

impl Msmpr {
    /// Bit enabling multi-core cache snoop coherency.
    pub const ENABLE: usize = 1;
}
