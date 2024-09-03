use core::{
    cell::UnsafeCell,
    hint::spin_loop,
    sync::atomic::{AtomicUsize, Ordering},
};
use rustsbi::{spec::hsm::hart_state, SbiRet};
use riscv::register::mstatus::MPP;

use crate::{NextStage,current_hartid};
use crate::trap_stack::ROOT_STACK;
use crate::clint;

const HART_STATE_START_PENDING_EXT: usize = usize::MAX;


type HsmState = AtomicUsize;

pub(crate) struct HsmCell<T> {
    status: HsmState,
    inner: UnsafeCell<Option<T>>,
}

impl<T> HsmCell<T> {
    /// 创建一个新的共享对象。
    pub const fn new() -> Self {
        Self {
            status: HsmState::new(hart_state::STOPPED),
            inner: UnsafeCell::new(None),
        }
    }

    /// 从当前硬件线程的状态中获取线程间共享对象。
    ///
    /// # Safety
    ///
    /// 用户需要确保对象属于当前硬件线程。
    #[inline]
    pub unsafe fn local(&self) -> LocalHsmCell<'_, T> {
        LocalHsmCell(self)
    }

    /// 取出共享对象。
    #[inline]
    pub fn remote(&self) -> RemoteHsmCell<'_, T> {
        RemoteHsmCell(self)
    }
}


/// 当前硬件线程的共享对象。
pub struct LocalHsmCell<'a, T>(&'a HsmCell<T>);

/// 任意硬件线程的共享对象。
pub struct RemoteHsmCell<'a, T>(&'a HsmCell<T>);

unsafe impl<T: Send> Sync for HsmCell<T> {}
unsafe impl<T: Send> Send for HsmCell<T> {}

impl<T> LocalHsmCell<'_, T> {
    /// 从启动挂起状态的硬件线程取出共享数据，并将其状态设置为启动，如果成功返回取出的数据，否则返回当前状态。
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

    /// 关闭。
    #[allow(unused)]
    #[inline]
    pub fn stop(&self) {
        self.0.status.store(hart_state::STOPPED, Ordering::Release)
    }

    /// 关闭。
    #[allow(unused)]
    #[inline]
    pub fn suspend(&self) {
        self.0
            .status
            .store(hart_state::SUSPENDED, Ordering::Relaxed)
    }

    /// 关闭。
    #[allow(unused)]
    #[inline]
    pub fn resume(&self) {
        self.0.status.store(hart_state::STARTED, Ordering::Relaxed)
    }
}

impl<T : core::fmt::Debug> RemoteHsmCell<'_, T> {
    /// 向关闭状态的硬件线程传入共享数据，并将其状态设置为启动挂起，返回是否放入成功。
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
            info!("t: {:?}",t);
            unsafe { *self.0.inner.get() = Some(t) };
            self.0
                .status
                .store(hart_state::START_PENDING, Ordering::Release);
            true
        } else {
            false
        }
    }

    /// 取出当前状态。
    #[allow(unused)]
    #[inline]
    pub fn sbi_get_status(&self) -> usize {
        match self.0.status.load(Ordering::Relaxed) {
            HART_STATE_START_PENDING_EXT => hart_state::START_PENDING,
            normal => normal,
        }
    }

    /// 判断这个 HART 能否接收 IPI。
    #[allow(unused)]
    #[inline]
    pub fn allow_ipi(&self) -> bool {
        matches!(
            self.0.status.load(Ordering::Relaxed),
            hart_state::STARTED | hart_state::SUSPENDED
        )
    }
}


/// 获取此 hart 的 local hsm 对象。
pub(crate) fn local_hsm() -> LocalHsmCell<'static, NextStage> {
    unsafe {
        ROOT_STACK
            .get_unchecked_mut(current_hartid())
            .hart_context()
            .hsm
            .local()
    }
}

/// 获取此 hart 的 remote hsm 对象。
pub(crate) fn local_remote_hsm() -> RemoteHsmCell<'static, NextStage> {
    unsafe {
        ROOT_STACK
            .get_unchecked_mut(current_hartid())
            .hart_context()
            .hsm
            .remote()
    }
}

/// 获取任意 hart 的 remote hsm 对象。
#[allow(unused)]
pub(crate) fn remote_hsm(hart_id: usize) -> Option<RemoteHsmCell<'static, NextStage>> {
    unsafe {
        ROOT_STACK
            .get_mut(hart_id)
            .map(|x| x.hart_context().hsm.remote())
    }
}



/// HSM 
pub(crate) struct Hsm;

impl rustsbi::Hsm for Hsm {
    fn hart_start(&self, hartid: usize, start_addr: usize, opaque: usize) -> SbiRet {
        match remote_hsm(hartid) {
            Some(remote) => {
                if remote.start(NextStage { start_addr, opaque, next_mode: MPP::Supervisor }) {
                    clint::set_msip(hartid);
                    SbiRet::success(0)
                } else {
                    SbiRet::already_started()
                }
            }
            None => SbiRet::invalid_param(),
        }
    }

    #[inline]
    fn hart_stop(&self) -> SbiRet {
        local_hsm().stop();
        SbiRet::success(0)
    }

    #[inline]
    fn hart_get_status(&self, hartid: usize) -> SbiRet {
        match remote_hsm(hartid) {
            Some(remote) => SbiRet::success(remote.sbi_get_status()),
            None => SbiRet::invalid_param(),
        }
    }

    fn hart_suspend(&self, suspend_type: u32, _resume_addr: usize, _opaque: usize) -> SbiRet {
        use rustsbi::spec::hsm::suspend_type::{NON_RETENTIVE, RETENTIVE};
        if matches!(suspend_type, NON_RETENTIVE | RETENTIVE) {
            local_hsm().suspend();
            riscv::asm::wfi() ;
            local_hsm().resume();
            SbiRet::success(0)
        } else {
            SbiRet::not_supported()
        }
    }
}
