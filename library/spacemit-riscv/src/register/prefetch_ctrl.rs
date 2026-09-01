//! Prefetch control register.
//!
//! # Platform support
//!
//! Public K3 firmware declares this register for SpacemiT X100 and A100 cores.
//! It accesses `l2_perf_dist` only on A100; behavior on X100 has not been
//! independently confirmed.
//!
//! Public K3 firmware accesses this register only on A100 harts. Its field
//! meaning and observed encoding are inferred from firmware comments.
//!
//! [K3 OpenSBI source](https://github.com/spacemit-com/opensbi/blob/8bd2cbdf9856dbc1a990d36e26bf47411f356c42/platform/generic/spacemit/spacemit_k3.c#L217-L230)

/// L2 prefetch-distance encoding for 56 entries.
///
/// This value is inferred from a public firmware comment.
pub const L2_PERF_DIST_56: usize = 3;

riscv::read_write_csr! {
    /// K3 A100 prefetch control register.
    PrefetchCtrl: 0x7d1,
    mask: 0x0c00,
}

riscv::read_write_csr_field! {
    PrefetchCtrl,
    /// L2 prefetch-distance encoding.
    ///
    /// This meaning is inferred from public K3 A100 firmware.
    l2_perf_dist: [10:11],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefetch_ctrl_field() {
        let mut value = PrefetchCtrl::from_bits(usize::MAX);
        assert_eq!(PrefetchCtrl::BITMASK, 0x0c00);
        assert_eq!(value.l2_perf_dist(), 3);

        value.set_l2_perf_dist(0);
        assert_eq!(value.bits(), 0);

        value.set_l2_perf_dist(L2_PERF_DIST_56);
        assert_eq!(value.l2_perf_dist(), L2_PERF_DIST_56);
        assert_eq!(value.bits(), 0x0c00);
    }
}
