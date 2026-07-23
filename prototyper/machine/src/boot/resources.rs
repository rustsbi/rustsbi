//! Ownership of validated boot resources and machine-only address claims.

use core::ops::Range;

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;

use super::{BootDtb, BootInfoError, NextStage};
#[cfg(test)]
use super::{BootDtbStorage, NextMode};
use crate::counter::{CounterError, PerformanceCounters};
use crate::hart::HartAdmission;
use crate::timer::Operations as TimerOperations;

/// The complete owned input delivered exactly once to upper firmware policy.
pub struct BootInfo {
    pub(super) dtb: BootDtb,
    pub(super) next_stage: NextStage,
    // Kept private so upper policy cannot mistake a physical hart ID for a
    // storage index. Final machine handoff uses it to prove that the hart map
    // contains the initializer before publishing warm-hart state.
    pub(super) init_hart: usize,
    pub(super) machine_ranges: Vec<Range<usize>>,
    pub(super) hart_admission: Option<Arc<HartAdmission>>,
    pub(super) timer: Option<&'static TimerOperations>,
    pub(super) counters: Option<PerformanceCounters>,
}

impl BootInfo {
    /// Publishes the complete machine runtime and enters the next stage.
    ///
    /// This consuming method is the sole public terminal handoff. Every
    /// fallible preparation step completes before secondary harts can observe
    /// the runtime.
    pub fn enter_next_stage(self, handler: Box<dyn crate::SbiHandler>) -> ! {
        super::prepare::enter_next_stage(self, handler)
    }

    /// Borrows the owned DTB for platform discovery and visibility policy.
    pub fn dtb_mut(&mut self) -> &mut BootDtb {
        &mut self.dtb
    }

    pub(super) fn new(
        dtb: BootDtb,
        next_stage: NextStage,
        init_hart: usize,
    ) -> Result<Self, BootInfoError> {
        let boot = Self {
            dtb,
            next_stage,
            init_hart,
            machine_ranges: Vec::new(),
            hart_admission: None,
            timer: None,
            counters: None,
        };
        if boot.invariants_hold() {
            Ok(boot)
        } else {
            Err(BootInfoError::InvalidOwnedState)
        }
    }

    fn invariants_hold(&self) -> bool {
        !self.dtb.as_bytes().is_empty() && self.next_stage.invariants_hold()
    }

    pub(crate) fn dtb(&self) -> &BootDtb {
        &self.dtb
    }

    pub(crate) fn init_hart_id(&self) -> usize {
        self.init_hart
    }

    /// Creates the performance-counter capability used by upper SBI policy.
    ///
    /// The returned handle has no usable counter identities until terminal
    /// machine preparation installs the current hart's private trap state, probes the
    /// hardware, and publishes the closed lower-privilege access policy.
    pub fn performance_counters(&mut self) -> Result<PerformanceCounters, CounterError> {
        if let Some(counters) = &self.counters {
            return Ok(counters.share());
        }
        let counters = PerformanceCounters::unprepared()?;
        self.counters = Some(counters.share());
        Ok(counters)
    }

    /// Builds bounded access to supervisor-visible physical memory.
    pub fn supervisor_memory(
        &self,
    ) -> Result<crate::memory::SupervisorMemory, crate::memory::MemoryError> {
        crate::memory::SupervisorMemory::from_boot(self)
    }

    pub(crate) fn machine_ranges(&self) -> &[Range<usize>] {
        &self.machine_ranges
    }

    pub(crate) fn ensure_runtime_unbound(&self) -> Result<(), RuntimeInstallError> {
        if self.hart_admission.is_some() || self.timer.is_some() {
            Err(RuntimeInstallError::AlreadyInstalled)
        } else {
            Ok(())
        }
    }

    pub(crate) fn install_runtime(
        &mut self,
        admission: Arc<HartAdmission>,
        timer: &'static TimerOperations,
    ) -> Result<(), RuntimeInstallError> {
        self.ensure_runtime_unbound()?;
        self.hart_admission = Some(admission);
        self.timer = Some(timer);
        Ok(())
    }

    pub(crate) fn claim_machine_range(
        &mut self,
        range: Range<usize>,
    ) -> Result<(), MachineRangeError> {
        self.claim_machine_ranges(core::slice::from_ref(&range))
    }

    pub(crate) fn claim_machine_ranges(
        &mut self,
        ranges: &[Range<usize>],
    ) -> Result<(), MachineRangeError> {
        for (index, range) in ranges.iter().enumerate() {
            if range.start >= range.end
                || !range.start.is_multiple_of(4)
                || !range.end.is_multiple_of(4)
            {
                return Err(MachineRangeError::Invalid);
            }
            if self
                .machine_ranges
                .iter()
                .any(|claimed| overlaps(range, claimed))
                || ranges[..index].iter().any(|other| overlaps(range, other))
            {
                return Err(MachineRangeError::AlreadyClaimed);
            }
        }
        self.machine_ranges.extend(ranges.iter().cloned());
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn from_test_dtb(bytes: Vec<u8>) -> Self {
        Self {
            dtb: BootDtb {
                storage: BootDtbStorage::Encoded(bytes),
            },
            next_stage: NextStage {
                entry: 0x8020_0000,
                opaque: 0,
                mode: NextMode::Supervisor,
            },
            init_hart: 0,
            machine_ranges: Vec::new(),
            hart_admission: None,
            timer: None,
            counters: None,
        }
    }
}

fn overlaps(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MachineRangeError {
    Invalid,
    AlreadyClaimed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RuntimeInstallError {
    AlreadyInstalled,
}
