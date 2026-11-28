//! SBI Hart State Management adapter.
//!
//! The SBI ABI and platform enabled-hart policy live here.
//! Runtime owns the protocol-free hart state machine and all privileged
//! transitions.
//!
//! # References
//!
//! - Specification: [RISC-V SBI HSM extension](https://docs.riscv.org/reference/sbi/v3.0/ext-hsm.html) —
//!   hart states and start, stop, and suspend transitions.

#![forbid(unsafe_code)]

use runtime::{
    Result,
    hart::{self, HartId, HartState, Xlen},
    memory::{PhysAddr, PhysAddrRange, SupervisorMemory},
    rustsbi::{Hsm, SbiRet},
    timer,
};
use sbi_spec::hsm::{hart_state, suspend_type};

/// SBI HSM extension composed from platform policy and Runtime hart mechanism.
pub(crate) struct SbiHsm {
    hardware_wakeup: bool,
    memory: &'static SupervisorMemory,
}

impl SbiHsm {
    /// Creates an HSM extension with the platform's hardware wakeup capability.
    pub(crate) fn new(hardware_wakeup: bool, memory: &'static SupervisorMemory) -> Self {
        Self {
            hardware_wakeup,
            memory,
        }
    }

    /// Validates an enabled SBI hart ID against platform privilege policy.
    fn hart_id(&self, raw: usize) -> Result<HartId, SbiRet> {
        let hart = HartId::from_raw(raw).map_err(|_| SbiRet::invalid_param())?;
        if !(self.hardware_wakeup || super::features::hart_privilege_checked(hart)) {
            return Err(SbiRet::invalid_param());
        }
        Ok(hart)
    }

    fn check_entry(&self, hart: HartId, address: usize) -> Result<(), SbiRet> {
        let alignment = super::features::instruction_alignment(hart);
        if !address.is_multiple_of(alignment)
            || (address > u32::MAX as usize
                && super::features::supervisor_xlen(hart) != Some(Xlen::X64))
        {
            return Err(SbiRet::invalid_address());
        }
        let range = PhysAddrRange::from_start_len(PhysAddr::new(address), alignment)
            .map_err(|_| SbiRet::invalid_address())?;
        // The aligned minimum instruction occupies a single four-byte PMP unit.
        (self.memory.contains_entry(range)
            && crate::firmware::permits_instruction_fetch(range.start()))
        .then_some(())
        .ok_or_else(SbiRet::invalid_address)
    }

    /// Implements the SBI HSM suspend/resume transition.
    fn suspend(&self, suspend_type: u32, resume_addr: usize, opaque: usize) -> SbiRet {
        if !matches!(
            suspend_type,
            suspend_type::RETENTIVE | suspend_type::NON_RETENTIVE
        ) {
            return SbiRet::invalid_param();
        }
        if suspend_type == suspend_type::NON_RETENTIVE {
            let Ok(hart) = HartId::current() else {
                return SbiRet::failed();
            };
            if let Err(error) = self.check_entry(hart, resume_addr) {
                return error;
            }
        }
        if hart::suspend_current().is_err() {
            return SbiRet::failed();
        }

        if suspend_type == suspend_type::RETENTIVE {
            return match hart::resume_current_retentive() {
                Ok(()) => SbiRet::success(0),
                Err(_) => SbiRet::failed(),
            };
        }

        let ticket = match hart::begin_nonretentive_resume(resume_addr, opaque) {
            Ok(ticket) => ticket,
            Err(_) => return SbiRet::failed(),
        };

        crate::sbi::pmu::reset_current();
        ticket.commit();
        SbiRet::success(0)
    }
}

impl Hsm for SbiHsm {
    /// Starts execution on a stopped hart.
    fn hart_start(&self, hartid: usize, start_addr: usize, opaque: usize) -> SbiRet {
        let hart = match self.hart_id(hartid) {
            Ok(hart) => hart,
            Err(error) => return error,
        };
        if let Err(error) = self.check_entry(hart, start_addr) {
            return error;
        }
        match hart::start(hart, start_addr, opaque) {
            Ok(hart::StartOutcome::Accepted) => SbiRet::success(0),
            Ok(hart::StartOutcome::AlreadyRunning) => SbiRet::already_available(),
            Err(_) => SbiRet::failed(),
        }
    }

    /// Stops execution on the current hart.
    #[inline]
    fn hart_stop(&self) -> SbiRet {
        if timer::get()
            .and_then(|timer| match timer {
                Some(timer) => timer.set_deadline(u64::MAX),
                None => Ok(()),
            })
            .is_err()
        {
            return SbiRet::failed();
        }
        match hart::stop_current() {
            Ok(never) => match never {},
            Err(_) => SbiRet::failed(),
        }
    }

    /// Returns the current state of a hart.
    #[inline]
    fn hart_get_status(&self, hartid: usize) -> SbiRet {
        let hart = match self.hart_id(hartid) {
            Ok(hart) => hart,
            Err(error) => return error,
        };
        let state = match hart::status(hart) {
            HartState::Started => hart_state::STARTED,
            HartState::Stopped => hart_state::STOPPED,
            HartState::StartPending => hart_state::START_PENDING,
            HartState::Suspended => hart_state::SUSPENDED,
            HartState::ResumePending => hart_state::RESUME_PENDING,
        };
        SbiRet::success(state)
    }

    /// Suspends and resumes the current hart through Runtime's mechanism.
    #[inline]
    fn hart_suspend(&self, suspend_type: u32, resume_addr: usize, opaque: usize) -> SbiRet {
        self.suspend(suspend_type, resume_addr, opaque)
    }
}
