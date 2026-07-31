//! Owned boot inputs and their raw import boundary.

// Production ownership is entered only by the RISC-V raw entry.

use core::ops::Range;

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::hart::HartAdmission;
use crate::pmu::{CounterError, PerformanceCounters};
use crate::timer::Operations as TimerOperations;

mod dtb;
mod handoff;
mod prepare;

pub use dtb::BootDtb;
pub(crate) use dtb::copy_from_entry;
#[cfg(test)]
use dtb::{BootDtbImportError, BootDtbStorage, DTB_HEADER_SIZE, DTB_MAGIC, validate_envelope};
#[cfg(test)]
use handoff::encode_for_test;
/// A validated privilege transition prepared from the selected boot protocol.
pub struct NextStage {
    pub(super) entry: usize,
    pub(super) opaque: usize,
    pub(super) mode: NextMode,
}

impl NextStage {
    pub(crate) const fn new(entry: usize, opaque: usize, mode: NextMode) -> Self {
        Self {
            entry,
            opaque,
            mode,
        }
    }

    /// Validates a supervisor entry address and its opaque ABI argument.
    pub fn supervisor(entry: usize, opaque: usize) -> Result<Self, crate::HartError> {
        if !crate::config::next_address_allowed(entry) || !entry.is_multiple_of(2) {
            return Err(crate::HartError::InvalidAddress);
        }
        Ok(Self {
            entry,
            opaque,
            mode: NextMode::Supervisor,
        })
    }

    pub(crate) fn into_parts(self) -> (usize, usize, NextMode) {
        (self.entry, self.opaque, self.mode)
    }

    /// Performs the one architectural return into this already-validated stage.
    pub(crate) fn transfer(self, hart_id: usize, opaque_override: Option<usize>) -> ! {
        const MSTATUS_MPP: usize = 0b11 << 11;
        const MSTATUS_MPIE: usize = 1 << 7;
        const MSTATUS_MPRV: usize = 1 << 17;
        const SSTATUS_SIE: usize = 1 << 1;

        if crate::power::is_terminal() {
            crate::power::halt();
        }
        let (entry, opaque, mode) = self.into_parts();
        let opaque = opaque_override.unwrap_or(opaque);
        let clear = MSTATUS_MPP | MSTATUS_MPIE | MSTATUS_MPRV | SSTATUS_SIE;
        let set = (mode as usize) << 11 | MSTATUS_MPIE;
        // SAFETY: boot preparation established the target mode, entry,
        // handoff DTB, trap state, delegation, and PMP policy. Rust reaches
        // no owner after this non-returning machine return.
        unsafe {
            core::arch::asm!(
                "csrc mstatus, {clear}",
                "csrs mstatus, {set}",
                "csrw satp, zero",
                "sfence.vma",
                "csrw mepc, {entry}",
                "mv a0, {hart_id}",
                "mv a1, {opaque}",
                "mv a2, zero",
                "mret",
                clear = in(reg) clear,
                set = in(reg) set,
                entry = in(reg) entry,
                hart_id = in(reg) hart_id,
                opaque = in(reg) opaque,
                options(noreturn),
            )
        }
    }

    pub(crate) const fn mode(&self) -> NextMode {
        self.mode
    }

    #[cfg(test)]
    pub(crate) fn for_test(entry: usize) -> Self {
        Self {
            entry,
            opaque: 0,
            mode: NextMode::Supervisor,
        }
    }
}

/// A validated next-stage privilege mode.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum NextMode {
    /// User mode.
    User = 0,
    /// Supervisor mode.
    Supervisor = 1,
    /// Machine mode.
    Machine = 3,
}

/// The complete owned input delivered exactly once to upper firmware policy.
pub struct BootInfo {
    pub(super) dtb: BootDtb,
    pub(super) next_stage: NextStage,
    pub(super) init_hart: usize,
    pub(super) harts: Option<Box<[usize]>>,
    pub(super) machine_ranges: Vec<Range<usize>>,
    pub(super) hart_admission: Option<Arc<HartAdmission>>,
    pub(super) timer: Option<&'static TimerOperations>,
    pub(super) counters: Option<PerformanceCounters>,
    pub(super) protection: Option<crate::pmp::Configuration>,
}

impl BootInfo {
    /// Publishes the complete machine runtime and enters the next stage.
    pub fn enter_next_stage(self, handler: Box<dyn crate::SbiHandler>) -> ! {
        prepare::enter_next_stage(self, handler)
    }

    /// Borrows the owned DTB for firmware discovery and visibility policy.
    pub fn dtb_mut(&mut self) -> &mut BootDtb {
        &mut self.dtb
    }

    /// Records the complete firmware-selected hart set for machine setup.
    ///
    /// Device-tree interpretation belongs to firmware policy. The machine
    /// runtime receives only the checked architectural hart identities it must
    /// admit, stack, and configure.
    pub fn set_harts(&mut self, harts: &[usize]) -> bool {
        if self.harts.is_some()
            || harts.is_empty()
            || harts.len() > crate::config::HART_CAPACITY
            || !harts.contains(&self.init_hart)
            || harts
                .iter()
                .enumerate()
                .any(|(index, hart)| harts[..index].contains(hart))
        {
            return false;
        }
        self.harts = Some(harts.to_vec().into_boxed_slice());
        true
    }

    pub(crate) fn new(dtb: BootDtb, next_stage: NextStage, init_hart: usize) -> Self {
        Self {
            dtb,
            next_stage,
            init_hart,
            harts: None,
            machine_ranges: Vec::new(),
            hart_admission: None,
            timer: None,
            counters: None,
            protection: None,
        }
    }

    pub(crate) fn init_hart_id(&self) -> usize {
        self.init_hart
    }

    /// Creates the performance-counter capability used by upper SBI policy.
    pub fn performance_counters(&mut self) -> Result<PerformanceCounters, CounterError> {
        if let Some(counters) = &self.counters {
            return Ok(counters.share());
        }
        let counters = PerformanceCounters::unprepared()?;
        self.counters = Some(counters.share());
        Ok(counters)
    }

    /// Selects the immutable lower-privilege physical-memory policy.
    pub fn set_memory_protection(&mut self, configuration: crate::pmp::Configuration) -> bool {
        if self.protection.is_some() {
            return false;
        }
        self.protection = Some(configuration);
        true
    }

    /// Builds bounded access to firmware-selected supervisor RAM ranges.
    pub fn supervisor_memory(
        &self,
        ranges: &[Range<usize>],
    ) -> Result<crate::memory::SupervisorMemory, crate::memory::MemoryError> {
        crate::memory::SupervisorMemory::from_boot(self, ranges)
    }

    pub(crate) fn machine_ranges(&self) -> &[Range<usize>] {
        &self.machine_ranges
    }

    pub(crate) fn install_runtime(
        &mut self,
        admission: Arc<HartAdmission>,
        timer: &'static TimerOperations,
    ) -> bool {
        if self.hart_admission.is_some() || self.timer.is_some() {
            return false;
        }
        self.hart_admission = Some(admission);
        self.timer = Some(timer);
        true
    }

    pub(crate) fn claim_machine_range(&mut self, range: Range<usize>) -> bool {
        if range.start >= range.end
            || !range.start.is_multiple_of(4)
            || !range.end.is_multiple_of(4)
            || self
                .machine_ranges
                .iter()
                .any(|claimed| range.start < claimed.end && claimed.start < range.end)
        {
            return false;
        }
        self.machine_ranges.push(range);
        true
    }

    #[cfg(test)]
    pub(crate) fn from_test_dtb(bytes: Vec<u8>) -> Self {
        Self {
            dtb: BootDtb {
                storage: BootDtbStorage::Encoded(bytes),
            },
            next_stage: NextStage::for_test(0x8020_0000),
            init_hart: 0,
            harts: None,
            machine_ranges: Vec::new(),
            hart_admission: None,
            timer: None,
            counters: None,
            protection: None,
        }
    }
}

#[cfg(test)]
mod tests;
