//! Pending remote fences and completion acknowledgements.

#![forbid(unsafe_code)]

use alloc::collections::VecDeque;
use core::sync::atomic::{AtomicBool, AtomicU32};
use runtime::{
    fence::LocalFence,
    hart::{HartId, PerHart},
};
use spin::{Mutex, Once};

/// Capacity of the per-hart fence operation queue.
const QUEUE_CAP: usize = 16;

/// Requests sent to this hart, kept separate from outgoing completions.
// Alignment reduces false sharing; it does not guarantee cache-line isolation.
#[repr(align(128))]
struct RequestQueue {
    entries: Mutex<VecDeque<(LocalFence, HartId)>>,
}

impl RequestQueue {
    fn new() -> Self {
        Self {
            entries: Mutex::new(VecDeque::new()),
        }
    }

    fn try_push(&self, item: (LocalFence, HartId)) -> bool {
        // Give the caller a chance to service local work when a remote
        // queue is busy; spinning on its lock can block mutual progress.
        let Some(mut q) = self.entries.try_lock() else {
            return false;
        };
        if q.len() >= QUEUE_CAP {
            return false;
        }
        q.push_back(item);
        true
    }

    fn pop(&self) -> Option<(LocalFence, HartId)> {
        self.entries.lock().pop_front()
    }

    fn cancel(&self, source_hart: HartId) -> bool {
        let mut queue = self.entries.lock();
        let Some(index) = queue.iter().position(|(_, source)| *source == source_hart) else {
            return false;
        };
        queue.remove(index);
        true
    }
}

/// Outstanding requests originating from this hart.
// Keep completion updates separate from incoming queue updates for performance.
#[repr(align(128))]
struct CompletionTracker {
    pending: AtomicU32,
    failed: AtomicBool,
}

impl CompletionTracker {
    fn new() -> Self {
        Self {
            pending: AtomicU32::new(0),
            failed: AtomicBool::new(false),
        }
    }

    fn begin(&self) -> bool {
        use core::sync::atomic::Ordering;
        if !self.is_sync() {
            return false;
        }
        self.failed.store(false, Ordering::Relaxed);
        true
    }

    fn is_sync(&self) -> bool {
        use core::sync::atomic::Ordering;
        if self.pending.load(Ordering::Relaxed) != 0 {
            return false;
        }
        core::sync::atomic::fence(Ordering::Acquire);
        true
    }

    fn add(&self) {
        use core::sync::atomic::Ordering;
        self.pending.fetch_add(1, Ordering::Relaxed);
    }

    fn complete(&self, success: bool) {
        use core::sync::atomic::Ordering;
        if !success {
            self.failed.store(true, Ordering::Relaxed);
        }
        // The sender must observe every receiver's result when pending reaches
        // zero. Its acquire fence pairs with these release decrements.
        self.pending.fetch_sub(1, Ordering::Release);
    }

    fn failed(&self) -> bool {
        use core::sync::atomic::Ordering;
        self.failed.load(Ordering::Relaxed)
    }
}

/// State for remote fence operations involving one hart.
pub(crate) struct RFenceCell {
    queue: RequestQueue,
    completions: CompletionTracker,
}

impl RFenceCell {
    fn new() -> Self {
        Self {
            queue: RequestQueue::new(),
            completions: CompletionTracker::new(),
        }
    }

    /// Returns a local view of this fence cell.
    #[inline]
    pub fn local(&self) -> LocalRFenceCell<'_> {
        LocalRFenceCell(self)
    }

    /// Returns a remote view of this fence cell.
    #[inline]
    pub fn remote(&self) -> RemoteRFenceCell<'_> {
        RemoteRFenceCell(self)
    }
}

/// Fence queues and synchronization counters for every hart in the boot topology.
static RFENCE_CELLS: Once<PerHart<RFenceCell>> = Once::new();

/// Creates one fence cell per enabled hart on the boot hart.
///
/// The caller must run this after Runtime publishes the hart topology and
/// before any hart enqueues or services a remote fence.
pub(super) fn init() {
    RFENCE_CELLS.call_once(|| {
        PerHart::new(|_| RFenceCell::new())
            .expect("BUG: per-hart RFENCE state requires a published hart topology")
    });
}

/// Borrows a validated hart's fence cell.
pub(super) fn rfence_cell(hart: HartId) -> &'static RFenceCell {
    RFENCE_CELLS
        .get()
        .expect("BUG: per-hart RFENCE state used before initialization")
        .get(hart)
}

/// A view of an [`RFenceCell`] for operations on the current hart.
pub struct LocalRFenceCell<'a>(&'a RFenceCell);

/// A view of an [`RFenceCell`] for operations from other harts.
pub struct RemoteRFenceCell<'a>(&'a RFenceCell);

impl LocalRFenceCell<'_> {
    /// Starts an outgoing operation if all previous requests have been acknowledged.
    pub fn begin(&self) -> bool {
        self.0.completions.begin()
    }

    /// Acknowledges an outgoing request canceled before its receiver took it.
    pub fn cancel_completion(&self) {
        self.0.completions.complete(false);
    }

    /// Checks if all synchronization operations are complete.
    pub fn is_sync(&self) -> bool {
        self.0.completions.is_sync()
    }

    /// Increments the synchronization counter.
    pub fn add(&self) {
        self.0.completions.add();
    }

    /// Reports an execution or delivery failure after [`Self::is_sync`] succeeds.
    pub fn failed(&self) -> bool {
        self.0.completions.failed()
    }

    /// Returns the next fence operation from the queue, if any.
    pub fn get(&self) -> Option<(LocalFence, HartId)> {
        self.0.queue.pop()
    }
}

impl RemoteRFenceCell<'_> {
    /// Cancels this source's queued request after an IPI send failure.
    pub(crate) fn cancel(&self, source_hart: HartId) -> bool {
        self.0.queue.cancel(source_hart)
    }

    /// Attempts to enqueue a fence operation, returning whether it succeeded.
    #[inline]
    pub(crate) fn try_push(&self, item: (LocalFence, HartId)) -> bool {
        self.0.queue.try_push(item)
    }

    /// Acknowledges the request and makes its result visible to the sender.
    pub fn complete(&self, success: bool) {
        self.0.completions.complete(success);
    }
}
