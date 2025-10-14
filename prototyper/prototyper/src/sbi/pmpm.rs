use crate::platform::PLATFORM;
use crate::riscv::current_hartid;
use crate::sbi::fifo::{Fifo, FifoError};
use crate::sbi::trap_stack::ROOT_STACK;
use core::sync::atomic::{AtomicU32, Ordering};
use pmpm::{PmpSlice, set_pmp_entry};
use riscv::register::{Permission, Range};
use rustsbi::SbiRet;
use spin::mutex::Mutex;
/// Context information for a PMP sync operation.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PmpSyncContext {
    /// PMP addr info.
    slice: PmpSlice,
    /// PMP addr range mode.
    mode: Range,
    /// Permission for PMP entry, R/W/X/N
    perm: Permission,
    /// Index of PMP entry.
    idx: u8,
}

/// Cell for managing PMP sync operations between harts.
pub(crate) struct PmpSyncCell {
    // Queue of PMP operations.
    mailbox: Mutex<Fifo<(PmpSyncContext, usize)>>,
    // Wait for other cores complete.
    wait_sync_count: AtomicU32,
}

// Mark PmpSyncCell as safe to share between threads
unsafe impl Sync for PmpSyncCell {}
unsafe impl Send for PmpSyncCell {}

impl PmpSyncCell {
    #[allow(unused)]
    pub const fn new() -> Self {
        Self {
            mailbox: Mutex::new(Fifo::new()),
            wait_sync_count: AtomicU32::new(0),
        }
    }
    /// Gets a local view of this fence cell for the current hart.
    #[inline]
    pub fn local(&self) -> LocalPmpSyncCell<'_> {
        LocalPmpSyncCell(self)
    }
    /// Gets a remote view of this fence cell for accessing from other harts.
    #[inline]
    pub fn remote(&self) -> RemotePmpSyncCell<'_> {
        RemotePmpSyncCell(self)
    }
}

/// View of PmpSyncCell for operations on the current hart.
pub struct LocalPmpSyncCell<'a>(&'a PmpSyncCell);
/// View of PmpSyncCell for operations from other harts.
pub struct RemotePmpSyncCell<'a>(&'a PmpSyncCell);

impl LocalPmpSyncCell<'_> {
    /// Checks if all synchronization operations are complete.
    #[inline]
    pub fn is_sync(&self) -> bool {
        self.0.wait_sync_count.load(Ordering::Relaxed) == 0
    }

    /// Increments the synchronization counter.
    #[inline]
    pub fn add(&self) {
        self.0.wait_sync_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Checks if the PMP sync queue is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.mailbox.lock().is_empty()
    }

    /// Gets the next PMP sync operation from the queue of current hart.
    #[inline]
    pub fn get(&self) -> Option<(PmpSyncContext, usize)> {
        self.0.mailbox.lock().pop().ok()
    }

    /// Adds a PMP sync operation to the queue of current hart.
    #[allow(unused)]
    pub fn set(&self, ctx: PmpSyncContext) -> bool {
        loop {
            let mut mailbox = self.0.mailbox.lock();
            match mailbox.push((ctx, current_hartid())) {
                Ok(_) => return true,
                Err(FifoError::Full) => return false,
                Err(_) => panic!("Unable to push PMP sync ops to fifo"),
            }
        }
    }
}

#[allow(unused)]
impl RemotePmpSyncCell<'_> {
    /// Adds a PMP sync operation to the queue of a remote hart.
    pub fn set(&self, ctx: PmpSyncContext) -> bool {
        loop {
            let mut mailbox = self.0.mailbox.lock();
            match mailbox.push((ctx, current_hartid())) {
                Ok(_) => return true,
                Err(FifoError::Full) => return false,
                Err(_) => panic!("Unable to push PMP sync ops to fifo"),
            }
        }
    }

    /// Decrements the synchronization counter.
    pub fn sub(&self) {
        self.0.wait_sync_count.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Gets the local PMP sync queue for the current hart.
#[inline]
pub(crate) fn local_pmpctx() -> Option<LocalPmpSyncCell<'static>> {
    unsafe {
        ROOT_STACK
            .get_mut(current_hartid())
            .map(|x| x.hart_context().pmpsync.local())
    }
}

/// Gets the remote PMP sync context for a specific hart.
#[inline]
pub(crate) fn remote_pmpctx(hart_id: usize) -> Option<RemotePmpSyncCell<'static>> {
    unsafe {
        ROOT_STACK
            .get_mut(hart_id)
            .map(|x| x.hart_context().pmpsync.remote())
    }
}

#[inline]
pub(crate) fn pmpsync_handler() {
    let receiver = local_pmpctx().unwrap();
    while !receiver.is_empty() {
        if let Some(config) = receiver.get() {
            // Set local PMP entry.
            set_pmp_entry(config.0.idx, config.0.slice, config.0.mode, config.0.perm);
            // Notify sender process complete.
            if let Some(sender) = remote_pmpctx(config.1) {
                sender.sub();
            }
        }
    }
}

#[allow(unused)]
#[inline]
/// Set PMP entry @idx on every hart.
pub fn set_pmp_slot(idx: u8, slice: PmpSlice, mode: Range, perm: Permission) -> SbiRet {
    // Set other harts first.
    let sbi_ret = unsafe { PLATFORM.sbi.ipi.as_ref() }
        .unwrap()
        .send_ipi_by_pmpsync(PmpSyncContext {
            slice: slice,
            mode: (mode),
            perm: (perm),
            idx: (idx),
        });
    // If configure other harts successfully, then configure local PMP.
    set_pmp_entry(idx, slice, mode, perm);
    sbi_ret
}

#[allow(unused)]
#[inline]
/// Clean PMP entry @idx on every hart.
pub fn clean_pmp_slot(idx: u8) -> SbiRet {
    set_pmp_slot(idx, PmpSlice::new(0, 0, 0), Range::OFF, Permission::NONE)
}
