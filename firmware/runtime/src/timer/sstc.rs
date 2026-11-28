//! Current-hart supervisor timer comparison registers.

use crate::{Error, Result, irq};
use csr::{
    Read, Set, Write,
    standard::isa::privileged::{machine::Menvcfg, supervisor::Stimecmp},
};

/// Supervisor comparison registers detected on the current hart.
pub(super) struct Sstc(());

impl Sstc {
    pub(super) fn detect() -> Result<Option<Self>> {
        irq::with_local_disabled(|_| {
            if !crate::recovery::is_ready() {
                return Err(Error::NotReady);
            }
            // SAFETY: Runtime trap entry is ready, mscratch/MPRV are clear,
            // and local interrupts are excluded during the comparator probe.
            if unsafe { Stimecmp::try_read() }.is_err() {
                return Ok(None);
            }
            Ok(Some(Self(())))
        })
    }

    /// Sets `menvcfg.STCE` on this hart. S-mode access also requires `mcounteren.TM`.
    pub(super) fn enable_supervisor_access(&self) {
        // SAFETY: Sstc was detected on this hart. CSRRS changes only STCE.
        unsafe { Menvcfg::write(Set(Menvcfg::STCE)) };
    }

    /// Replaces the current hart's absolute supervisor deadline.
    ///
    /// `u64::MAX` cancels the deadline.
    /// A pending supervisor timer interrupt may take time to clear.
    pub(super) fn set_deadline(&self, deadline: u64) {
        irq::with_local_disabled(|_| {
            // SAFETY: Detection established the comparator on this hart.
            // Interrupts stay masked across the three RV32 comparator writes.
            unsafe { Stimecmp::write(deadline) };
        });
    }
}
