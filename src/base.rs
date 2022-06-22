#[inline]
pub fn probe_extension(extension: usize) -> bool {
    use sbi_spec::*;
    match extension {
        base::EID_BASE => true,
        time::EID_TIME => crate::timer::probe_timer(),
        spi::EID_SPI => crate::ipi::probe_ipi(),
        rfnc::EID_RFNC => crate::rfence::probe_rfence(),
        srst::EID_SRST => crate::reset::probe_reset(),
        hsm::EID_HSM => crate::hsm::probe_hsm(),
        pmu::EID_PMU => crate::pmu::probe_pmu(),
        _ => false,
    }
}
