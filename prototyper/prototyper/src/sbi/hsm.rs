use core::{
    cell::UnsafeCell,
    hint::spin_loop,
    sync::atomic::{AtomicUsize, Ordering},
};
use riscv::register::mstatus::MPP;
use rustsbi::{SbiRet, spec::hsm::hart_state};

use crate::platform::PLATFORM;
use crate::riscv::current_hartid;
use crate::sbi::hart_context::NextStage;
use crate::sbi::trap_stack::ROOT_STACK;
use crate::trap_stack::hart_context_mut;

use super::{trap::boot::boot, trap_stack::hart_context};

/// Special state indicating a hart is in the process of starting.
const HART_STATE_START_PENDING_EXT: usize = usize::MAX;

type HsmState = AtomicUsize;

/// Cell for managing hart state and shared data between harts.
pub(crate) struct HsmCell<T> {
    status: HsmState,
    inner: UnsafeCell<Option<T>>,
}

impl<T> HsmCell<T> {
    /// Creates a new HsmCell with STOPPED state and no inner data.
    pub const fn new() -> Self {
        Self {
            status: HsmState::new(hart_state::STOPPED),
            inner: UnsafeCell::new(None),
        }
    }

    /// Gets a local view of this cell for the current hart.
    ///
    /// # Safety
    ///
    /// Caller must ensure this cell belongs to the current hart.
    #[inline]
    pub unsafe fn local(&self) -> LocalHsmCell<'_, T> {
        LocalHsmCell(self)
    }

    /// Gets a remote view of this cell for accessing from other harts.
    #[inline]
    pub fn remote(&self) -> RemoteHsmCell<'_, T> {
        RemoteHsmCell(self)
    }
}

/// View of HsmCell for operations on the current hart.
pub struct LocalHsmCell<'a, T>(&'a HsmCell<T>);

/// View of HsmCell for operations from other harts.
pub struct RemoteHsmCell<'a, T>(&'a HsmCell<T>);

// Mark HsmCell as safe to share between threads
unsafe impl<T: Send> Sync for HsmCell<T> {}
unsafe impl<T: Send> Send for HsmCell<T> {}

impl<T> LocalHsmCell<'_, T> {
    /// Attempts to transition hart from START_PENDING to STARTED state.
    ///
    /// Returns inner data if successful, otherwise returns current state.
    #[inline]
    pub fn start(&self) -> Result<T, usize> {
        loop {
            match self.0.status.compare_exchange(
                hart_state::START_PENDING,
                hart_state::STARTED,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => break Ok(unsafe { (*self.0.inner.get()).take().unwrap() }),
                Err(HART_STATE_START_PENDING_EXT) => spin_loop(),
                Err(s) => break Err(s),
            }
        }
    }

    /// Transitions hart to STOPPED state.
    #[allow(unused)]
    #[inline]
    pub fn stop(&self) {
        self.0.status.store(hart_state::STOPPED, Ordering::Release)
    }

    /// Transitions hart to SUSPENDED state.
    #[allow(unused)]
    #[inline]
    pub fn suspend(&self) {
        self.0
            .status
            .store(hart_state::SUSPENDED, Ordering::Relaxed)
    }

    /// Transitions hart to STARTED state.
    #[allow(unused)]
    #[inline]
    pub fn resume(&self) {
        self.0.status.store(hart_state::STARTED, Ordering::Relaxed)
    }
}

impl<T: core::fmt::Debug> RemoteHsmCell<'_, T> {
    /// Attempts to start a stopped hart by providing startup data.
    ///
    /// Returns true if successful, false if hart was not in STOPPED state.
    #[inline]
    pub fn start(&self, t: T) -> bool {
        if self
            .0
            .status
            .compare_exchange(
                hart_state::STOPPED,
                HART_STATE_START_PENDING_EXT,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_ok()
        {
            unsafe { *self.0.inner.get() = Some(t) };
            self.0
                .status
                .store(hart_state::START_PENDING, Ordering::Release);
            true
        } else {
            false
        }
    }

    /// Attempts to resume a suspended hart by providing resume data.
    ///
    /// Returns true if successful, false if hart was not in SUSPENDED state.
    #[inline]
    pub fn resume(&self, t: T) -> bool {
        if self
            .0
            .status
            .compare_exchange(
                hart_state::SUSPENDED,
                HART_STATE_START_PENDING_EXT,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_ok()
        {
            unsafe { *self.0.inner.get() = Some(t) };
            self.0
                .status
                .store(hart_state::START_PENDING, Ordering::Release);
            true
        } else {
            false
        }
    }

    /// Gets the current state of the hart.
    #[allow(unused)]
    #[inline]
    pub fn get_status(&self) -> usize {
        match self.0.status.load(Ordering::Relaxed) {
            HART_STATE_START_PENDING_EXT => hart_state::START_PENDING,
            normal => normal,
        }
    }

    /// Checks if hart can receive IPIs (must be STARTED or SUSPENDED).
    #[allow(unused)]
    #[inline]
    pub fn allow_ipi(&self) -> bool {
        matches!(
            self.0.status.load(Ordering::Relaxed),
            hart_state::STARTED | hart_state::SUSPENDED
        )
    }
}

/// Gets the local HSM cell for the current hart.
pub(crate) fn local_hsm() -> LocalHsmCell<'static, NextStage> {
    unsafe { hart_context(current_hartid()).hsm.local() }
}

/// Gets a remote view of the current hart's HSM cell.
pub(crate) fn local_remote_hsm() -> RemoteHsmCell<'static, NextStage> {
    hart_context(current_hartid()).hsm.remote()
}

/// Gets a remote view of any hart's HSM cell.
#[allow(unused)]
pub(crate) fn remote_hsm(hart_id: usize) -> Option<RemoteHsmCell<'static, NextStage>> {
    unsafe {
        ROOT_STACK
            .get_mut(hart_id)
            .map(|x| x.hart_context().hsm.remote())
    }
}

/// Implementation of SBI HSM (Hart State Management) extension.
pub(crate) struct SbiHsm;

impl rustsbi::Hsm for SbiHsm {
    /// Starts execution on a stopped hart.
    fn hart_start(&self, hartid: usize, start_addr: usize, opaque: usize) -> SbiRet {
        match remote_hsm(hartid) {
            Some(remote) => {
                if remote.start(NextStage {
                    start_addr,
                    opaque,
                    next_mode: MPP::Supervisor,
                }) {
                    unsafe {
                        PLATFORM.sbi.ipi.as_ref().unwrap().set_msip(hartid);
                    }
                    SbiRet::success(0)
                } else {
                    SbiRet::already_available()
                }
            }
            None => SbiRet::invalid_param(),
        }
    }

    /// Stops execution on the current hart.
    #[inline]
    fn hart_stop(&self) -> SbiRet {
        local_hsm().stop();
        unsafe {
            riscv::register::mie::clear_msoft();
        }
        riscv::asm::wfi();
        SbiRet::success(0)
    }

    /// Gets the current state of a hart.
    #[inline]
    fn hart_get_status(&self, hartid: usize) -> SbiRet {
        match remote_hsm(hartid) {
            Some(remote) => SbiRet::success(remote.get_status()),
            None => SbiRet::invalid_param(),
        }
    }

    /// Suspends execution on the current hart.
    fn hart_suspend(&self, suspend_type: u32, resume_addr: usize, opaque: usize) -> SbiRet {
        use rustsbi::spec::hsm::suspend_type::{NON_RETENTIVE, RETENTIVE};

        if !matches!(suspend_type, NON_RETENTIVE | RETENTIVE) {
            return SbiRet::invalid_param();
        }

        crate::sbi::trap::handler::msoft_ipi_handler();
        unsafe {
            PLATFORM
                .sbi
                .ipi
                .as_ref()
                .unwrap()
                .clear_msip(current_hartid());
        }
        unsafe {
            riscv::register::mie::set_msoft();
        }
        local_hsm().suspend();
        riscv::asm::wfi();
        crate::sbi::trap::handler::msoft_ipi_handler();

        match suspend_type {
            RETENTIVE => {
                local_hsm().resume();
                return SbiRet::success(0);
            }
            NON_RETENTIVE => return self.hart_resume(current_hartid(), resume_addr, opaque),
            _ => return SbiRet::invalid_param(),
        }
    }
}

impl SbiHsm {
    // non retentive resume
    fn hart_resume(&self, hartid: usize, resume_addr: usize, opaque: usize) -> SbiRet {
        match remote_hsm(hartid) {
            Some(remote) => {
                if remote.resume(NextStage {
                    start_addr: resume_addr,
                    opaque,
                    next_mode: MPP::Supervisor,
                }) {
                    // reset the hart local context to prevent the hart context from being polluted
                    hart_context_mut(hartid).reset();
                    // boot resume hart from resume addr
                    unsafe {
                        boot();
                    }
                } else {
                    SbiRet::failed()
                }
            }
            None => SbiRet::failed(),
        }
    }
}
