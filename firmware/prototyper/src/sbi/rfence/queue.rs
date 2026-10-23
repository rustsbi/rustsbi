//! Pending remote fences and completion acknowledgements.

use super::{Request, rfence_poll};
use alloc::collections::VecDeque;
use core::sync::atomic::AtomicU32;
use runtime::hart::HartId;
use spin::Mutex;

/// Capacity of the per-hart fence operation queue.
const QUEUE_CAP: usize = 16;

#[repr(align(128))]
struct CacheAligned<T>(T);

impl<T> core::ops::Deref for CacheAligned<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

/// Cell for managing remote fence operations between harts.
pub(super) struct RFenceCell {
    // Queue of fence operations with source hart ID
    queue: CacheAligned<Mutex<VecDeque<(Request, HartId)>>>,
    // Counter for tracking pending synchronization operations
    wait_sync_count: CacheAligned<AtomicU32>,
}

impl RFenceCell {
    /// Creates a new RFenceCell with empty queue and zero sync count.
    pub(super) fn new() -> Self {
        Self {
            queue: CacheAligned(Mutex::new(VecDeque::new())),
            wait_sync_count: CacheAligned(AtomicU32::new(0)),
        }
    }

    /// Gets a local view of this fence cell for the current hart.
    #[inline]
    pub(super) fn local(&self) -> LocalRFenceCell<'_> {
        LocalRFenceCell(self)
    }

    /// Gets a remote view of this fence cell for accessing from other harts.
    #[inline]
    pub(super) fn remote(&self) -> RemoteRFenceCell<'_> {
        RemoteRFenceCell(self)
    }

    /// Pushes a fence operation into the queue, or reports it full.
    ///
    /// The retry policy on a full queue (drain via IPI) belongs to
    /// the rfence layer, which is why this returns the error instead of
    /// looping.
    fn try_push(&self, item: (Request, HartId)) -> bool {
        // Give the caller a chance to service local work when a remote
        // queue is busy; spinning on its lock can block mutual progress.
        let Some(mut q) = self.queue.try_lock() else {
            return false;
        };
        if q.len() >= QUEUE_CAP {
            return false;
        }
        q.push_back(item);
        true
    }
}

// RFenceCell is Sync+Send: Mutex<VecDeque<T>> is Sync when T: Send, AtomicU32 is Sync.

/// View of RFenceCell for operations on the current hart.
pub(super) struct LocalRFenceCell<'a>(&'a RFenceCell);

/// View of RFenceCell for operations from other harts.
pub(super) struct RemoteRFenceCell<'a>(&'a RFenceCell);

impl LocalRFenceCell<'_> {
    /// Checks if all synchronization operations are complete.
    pub(super) fn is_sync(&self) -> bool {
        use core::sync::atomic::Ordering;
        if self.0.wait_sync_count.load(Ordering::Relaxed) != 0 {
            return false;
        }
        core::sync::atomic::fence(Ordering::Acquire);
        true
    }

    /// Increments the synchronization counter.
    pub(super) fn add(&self) {
        use core::sync::atomic::Ordering;
        self.0.wait_sync_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Gets the next fence operation from the queue.
    pub(super) fn get(&self) -> Option<(Request, HartId)> {
        self.0.queue.lock().pop_front()
    }
}

impl RemoteRFenceCell<'_> {
    /// Cancels this source's queued request after an IPI send failure.
    pub(super) fn cancel(&self, source_hart: HartId) -> bool {
        let mut queue = self.0.queue.lock();
        let Some(index) = queue.iter().position(|(_, source)| *source == source_hart) else {
            return false;
        };
        queue.remove(index);
        true
    }

    /// Pushes a fence operation into the queue, or reports it full.
    #[inline]
    pub(super) fn try_push(&self, item: (Request, HartId)) -> bool {
        self.0.try_push(item)
    }

    /// Decrements the synchronization counter.
    pub(super) fn sub(&self) {
        use core::sync::atomic::Ordering;
        self.0.wait_sync_count.fetch_sub(1, Ordering::Release);
    }
}

impl RemoteRFenceCell<'_> {
    /// Adds a fence operation to the queue from a remote hart.
    pub(super) fn set(&self, ctx: Request) {
        let hart_id = HartId::current().expect("BUG: current hart exceeds Runtime capacity");
        loop {
            if self.try_push((ctx, hart_id)) {
                return;
            }
            rfence_poll();
        }
    }
}
