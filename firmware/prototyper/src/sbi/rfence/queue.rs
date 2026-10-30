//! Pending remote fences and completion acknowledgements.

use super::Request;
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
    // Incoming requests and the source hart that must receive completion.
    queue: CacheAligned<Mutex<VecDeque<(Request, HartId)>>>,
    // Outgoing requests from this hart that have not completed yet.
    pending_completions: CacheAligned<AtomicU32>,
}

impl RFenceCell {
    /// Creates a new RFenceCell with empty queue and zero sync count.
    pub(super) fn new() -> Self {
        Self {
            queue: CacheAligned(Mutex::new(VecDeque::new())),
            pending_completions: CacheAligned(AtomicU32::new(0)),
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
    /// The caller services local work before retrying a full or locked queue.
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

    fn complete_request(&self) {
        use core::sync::atomic::Ordering;
        let pending = self.pending_completions.fetch_sub(1, Ordering::Release);
        assert_ne!(
            pending, 0,
            "RFENCE completion without an outstanding request"
        );
    }
}

// RFenceCell is Sync+Send: Mutex<VecDeque<T>> is Sync when T: Send, AtomicU32 is Sync.

/// View of RFenceCell for operations on the current hart.
pub(super) struct LocalRFenceCell<'a>(&'a RFenceCell);

/// View of RFenceCell for operations from other harts.
pub(super) struct RemoteRFenceCell<'a>(&'a RFenceCell);

impl LocalRFenceCell<'_> {
    /// Checks whether every outgoing request has been acknowledged.
    pub(super) fn all_completed(&self) -> bool {
        use core::sync::atomic::Ordering;
        if self.0.pending_completions.load(Ordering::Relaxed) != 0 {
            return false;
        }
        core::sync::atomic::fence(Ordering::Acquire);
        true
    }

    /// Records one outgoing request before publishing it to the target.
    pub(super) fn expect_completion(&self) {
        use core::sync::atomic::Ordering;
        self.0.pending_completions.fetch_add(1, Ordering::Relaxed);
    }

    /// Removes a completion expectation after the queued request was cancelled.
    pub(super) fn cancel_completion(&self) {
        self.0.complete_request();
    }

    /// Gets the next fence operation from the queue.
    pub(super) fn pop_request(&self) -> Option<(Request, HartId)> {
        self.0.queue.lock().pop_front()
    }
}

impl RemoteRFenceCell<'_> {
    /// Cancels this source's queued request after an IPI send failure.
    pub(super) fn cancel_request_from(&self, source_hart: HartId) -> bool {
        let mut queue = self.0.queue.lock();
        let Some(index) = queue.iter().position(|(_, source)| *source == source_hart) else {
            return false;
        };
        queue.remove(index);
        true
    }

    /// Pushes a fence operation into the queue, or reports it full.
    #[inline]
    pub(super) fn try_enqueue(&self, request: Request, source: HartId) -> bool {
        self.0.try_push((request, source))
    }

    /// Acknowledges one request after executing its fence.
    pub(super) fn complete_request(&self) {
        self.0.complete_request();
    }
}
