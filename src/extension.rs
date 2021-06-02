use crate::ecall::*;

#[inline]
pub fn probe_extension(extension: usize) -> bool {
    match extension {
        EXTENSION_BASE => true,
        EXTENSION_TIMER => crate::timer::probe_timer(),
        EXTENSION_IPI => crate::ipi::probe_ipi(),
        EXTENSION_RFENCE => crate::rfence::probe_rfence(),
        EXTENSION_SRST => crate::reset::probe_reset(),
        EXTENSION_HSM => crate::hsm::probe_hsm(),
        // new extensions should be added here to be probed
        _ => false,
    }
}
