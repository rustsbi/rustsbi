//! Machine L2 hint register.
//!
//! # Platform support
//!
//! This register is supported on SpacemiT X100 and A100 cores. K3 OpenSBI
//! accesses `trace_top_icgen` on both core families and the two channel-2
//! fields only on A100. It declares `ciu_prf_throt_dis` without accessing it.
//!
//! All field meanings are inferred from public K3 firmware. The source comment
//! for `ciu_chr2_mer_dis` says bit 1 while its actual mask uses bit 2. The name
//! of `ciu_chr2_depd_dis` says that one disables dependency checking, but a
//! call site clears it while describing the check as disabled. The actual bit
//! positions are exposed here without resolving those documentation conflicts.
//!
//! [K3 OpenSBI source](https://github.com/spacemit-com/opensbi/blob/8bd2cbdf9856dbc1a990d36e26bf47411f356c42/platform/generic/spacemit/spacemit_k3.c#L217-L233)

riscv::read_write_csr! {
    /// Machine L2 hint register.
    Ml2hint: 0x7f7,
    mask: 0x0400_001c,
}

riscv::read_write_csr_field! {
    Ml2hint,
    /// CIU channel-2 merge-disable bit.
    ///
    /// This meaning is inferred from K3 firmware. Its source comment has a
    /// bit-number conflict with the implemented mask.
    ciu_chr2_mer_dis: 2,
}

riscv::read_write_csr_field! {
    Ml2hint,
    /// CIU channel-2 dependency-control bit.
    ///
    /// This meaning is inferred from K3 firmware, and its polarity is unresolved.
    ciu_chr2_depd_dis: 3,
}

riscv::read_write_csr_field! {
    Ml2hint,
    /// CIU prefetch-throttle-disable bit.
    ///
    /// This meaning is inferred only from a K3 firmware name.
    ciu_prf_throt_dis: 4,
}

riscv::read_write_csr_field! {
    Ml2hint,
    /// Trace top-level clock-gate enable.
    ///
    /// This meaning is inferred from public K3 firmware.
    trace_top_icgen: 26,
}

riscv::set!(0x7f7);
riscv::clear!(0x7f7);

riscv::set_clear_csr!(
    /// CIU channel-2 merge control.
    ///
    /// This meaning is inferred from public K3 firmware.
    , set_ciu_chr2_mer_dis, clear_ciu_chr2_mer_dis, 1 << 2);
riscv::set_clear_csr!(
    /// CIU channel-2 dependency control.
    ///
    /// This meaning is inferred from public K3 firmware, and its polarity is
    /// unresolved.
    , set_ciu_chr2_depd_dis, clear_ciu_chr2_depd_dis, 1 << 3);
riscv::set_clear_csr!(
    /// CIU prefetch-throttle control.
    ///
    /// This meaning is inferred from a public K3 firmware name.
    , set_ciu_prf_throt_dis, clear_ciu_prf_throt_dis, 1 << 4);
riscv::set_clear_csr!(
    /// Trace top-level clock-gate enable.
    ///
    /// This meaning is inferred from public K3 firmware.
    , set_trace_top_icgen, clear_trace_top_icgen, 1 << 26);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ml2hint_fields() {
        let mut value = Ml2hint::from_bits(0);
        assert_eq!(Ml2hint::BITMASK, 0x0400_001c);

        value.set_ciu_chr2_mer_dis(true);
        value.set_ciu_chr2_depd_dis(true);
        value.set_ciu_prf_throt_dis(true);
        value.set_trace_top_icgen(true);

        assert!(value.ciu_chr2_mer_dis());
        assert!(value.ciu_chr2_depd_dis());
        assert!(value.ciu_prf_throt_dis());
        assert!(value.trace_top_icgen());
        assert_eq!(value.bits(), 0x0400_001c);
    }
}
