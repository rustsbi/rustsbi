use crate::ecall::*;

#[inline]
pub fn probe_extension(extension: usize) -> bool {
    match extension {
        EXTENSION_BASE => true,
        EXTENSION_TIMER => crate::timer::probe_timer(),
        EXTENSION_IPI => crate::ipi::probe_ipi(),
        // new extensions should be added here to be probed
        _ => false,
    }
}
