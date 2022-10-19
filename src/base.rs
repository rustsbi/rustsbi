#[cfg(feature = "singleton")]
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
        // Legacy extensions
        // if feature 'legacy' is not enabled, these extensions fall back to false.
        #[cfg(feature = "legacy")]
        legacy::LEGACY_SET_TIMER => crate::timer::probe_timer(),
        #[cfg(feature = "legacy")]
        legacy::LEGACY_SEND_IPI => crate::ipi::probe_ipi(),
        // LEGACY_CLEAR_IPI implemented in ecall/mod.rs directly, so it always exists.
        #[cfg(feature = "legacy")]
        legacy::LEGACY_CLEAR_IPI => true,
        #[cfg(feature = "legacy")]
        legacy::LEGACY_SHUTDOWN => crate::reset::probe_reset(),
        // we don't include LEGACY_REMOTE_FENCE_I, LEGACY_REMOTE_SFENCE_VMA
        // and LEGACY_REMOTE_SFENCE_VMA_ASID here,
        // for RustSBI ecall/mod.rs did not implement these legacy extensions.
        #[cfg(feature = "legacy")]
        legacy::LEGACY_CONSOLE_PUTCHAR | legacy::LEGACY_CONSOLE_GETCHAR => {
            crate::legacy_stdio::probe_legacy_stdio()
        }
        _ => false,
    }
}
