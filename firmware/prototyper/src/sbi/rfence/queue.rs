//! Pending remote fences and completion acknowledgements.

use super::RFenceContext;
use crate::sbi::hart_local::CacheAligned;
use alloc::collections::VecDeque;
use core::sync::atomic::AtomicU32;
use spin::Mutex;

/// Capacity of the per-hart fence operation queue.
const QUEUE_CAP: usize = 16;

/// Cell for managing remote fence operations between harts.
pub(crate) struct RFenceCell {
    // Queue of fence operations with source hart ID
    queue: CacheAligned<Mutex<VecDeque<(RFenceContext, usize)>>>,
    // Counter for tracking pending synchronization operations
    wait_sync_count: CacheAligned<AtomicU32>,
}

impl RFenceCell {
    /// Creates a new RFenceCell with empty queue and zero sync count.
    pub fn new() -> Self {
        Self {
            queue: CacheAligned(Mutex::new(VecDeque::new())),
            wait_sync_count: CacheAligned(AtomicU32::new(0)),
        }
    }

    /// Gets a local view of this fence cell for the current hart.
    #[inline]
    pub fn local(&self) -> LocalRFenceCell<'_> {
        LocalRFenceCell(self)
    }

    /// Gets a remote view of this fence cell for accessing from other harts.
    #[inline]
    pub fn remote(&self) -> RemoteRFenceCell<'_> {
        RemoteRFenceCell(self)
    }

    /// Pushes a fence operation into the queue, or reports it full.
    ///
    /// The retry policy on a full queue (drain via IPI) belongs to
    /// the rfence layer, which is why this returns the error instead of
    /// looping.
    pub fn try_push(&self, item: (RFenceContext, usize)) -> bool {
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
pub struct LocalRFenceCell<'a>(&'a RFenceCell);

/// View of RFenceCell for operations from other harts.
pub struct RemoteRFenceCell<'a>(&'a RFenceCell);

impl LocalRFenceCell<'_> {
    /// Pushes a fence operation into the queue, or reports it full.
    #[inline]
    pub(crate) fn try_push(&self, item: (RFenceContext, usize)) -> bool {
        self.0.try_push(item)
    }

    /// Checks if all synchronization operations are complete.
    pub fn is_sync(&self) -> bool {
        use core::sync::atomic::Ordering;
        if self.0.wait_sync_count.load(Ordering::Relaxed) != 0 {
            return false;
        }
        core::sync::atomic::fence(Ordering::Acquire);
        true
    }

    /// Increments the synchronization counter.
    pub fn add(&self) {
        use core::sync::atomic::Ordering;
        self.0.wait_sync_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Gets the next fence operation from the queue.
    pub fn get(&self) -> Option<(RFenceContext, usize)> {
        self.0.queue.lock().pop_front()
    }
}

impl RemoteRFenceCell<'_> {
    /// Cancels this source's queued request after an IPI send failure.
    pub(crate) fn cancel(&self, source_hart: usize) -> bool {
        let mut queue = self.0.queue.lock();
        let Some(index) = queue.iter().position(|(_, source)| *source == source_hart) else {
            return false;
        };
        queue.remove(index);
        true
    }

    /// Pushes a fence operation into the queue, or reports it full.
    #[inline]
    pub(crate) fn try_push(&self, item: (RFenceContext, usize)) -> bool {
        self.0.try_push(item)
    }

    /// Decrements the synchronization counter.
    pub fn sub(&self) {
        use core::sync::atomic::Ordering;
        self.0.wait_sync_count.fetch_sub(1, Ordering::Release);
    }
}
