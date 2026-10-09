//! Pending remote fences and completion acknowledgements.

use crate::sbi::hart_local::CacheAligned;
use alloc::collections::VecDeque;
use core::sync::atomic::AtomicU32;
use runtime::rfence::{FenceError, FenceRequest};
use spin::Mutex;

/// Capacity of the per-hart fence operation queue.
const QUEUE_CAP: usize = 16;

/// Cell for managing remote fence operations between harts.
pub(crate) struct RFenceCell {
    // Incoming operations and their source hart IDs.
    queue: CacheAligned<Mutex<VecDeque<(FenceRequest, usize)>>>,
    // Outstanding acknowledgements for this hart's outgoing batch.
    wait_sync_count: CacheAligned<AtomicU32>,
    // Several receivers may report errors concurrently. Preserve the first
    // complete error, including architectural fault details.
    completion_error: CacheAligned<Mutex<Option<FenceError>>>,
}

impl RFenceCell {
    /// Creates a fence cell with no queued requests or outstanding acknowledgements.
    pub(crate) fn new() -> Self {
        Self {
            queue: CacheAligned(Mutex::new(VecDeque::new())),
            wait_sync_count: CacheAligned(AtomicU32::new(0)),
            completion_error: CacheAligned(Mutex::new(None)),
        }
    }

    /// Returns a view for operations by this cell's hart.
    #[inline]
    pub(crate) fn local(&self) -> LocalRFenceCell<'_> {
        LocalRFenceCell(self)
    }

    /// Returns a view for operations by other harts.
    #[inline]
    pub(crate) fn remote(&self) -> RemoteRFenceCell<'_> {
        RemoteRFenceCell(self)
    }

    /// Pushes a fence operation into the queue, or reports it full.
    ///
    /// The caller services its own incoming requests between retries.
    fn try_push(&self, item: (FenceRequest, usize)) -> bool {
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

/// View of [`RFenceCell`] for operations by its hart.
pub(crate) struct LocalRFenceCell<'a>(&'a RFenceCell);

/// View of [`RFenceCell`] for operations by other harts.
pub(crate) struct RemoteRFenceCell<'a>(&'a RFenceCell);

impl LocalRFenceCell<'_> {
    /// Starts the current hart's next batch after its previous acknowledgements.
    /// The SBI sender serializes batches on each source hart.
    pub(crate) fn begin_batch(&self) {
        assert!(
            self.is_sync(),
            "RFENCE batch still has outstanding requests"
        );
        *self.0.completion_error.lock() = None;
    }

    /// Takes the first receiver error after every submitted request completes.
    pub(crate) fn take_error(&self) -> Option<FenceError> {
        assert!(
            self.is_sync(),
            "RFENCE batch still has outstanding requests"
        );
        self.0.completion_error.lock().take()
    }

    /// Returns whether all outgoing requests have been acknowledged.
    pub(crate) fn is_sync(&self) -> bool {
        use core::sync::atomic::Ordering;
        if self.0.wait_sync_count.load(Ordering::Relaxed) != 0 {
            return false;
        }
        core::sync::atomic::fence(Ordering::Acquire);
        true
    }

    /// Adds one outstanding acknowledgement to the current batch.
    pub(crate) fn add(&self) {
        use core::sync::atomic::Ordering;
        self.0.wait_sync_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Removes and returns the next incoming fence request and its source hart.
    pub(crate) fn get(&self) -> Option<(FenceRequest, usize)> {
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
    pub(crate) fn try_push(&self, item: (FenceRequest, usize)) -> bool {
        self.0.try_push(item)
    }

    /// Records the receiver's result and acknowledges its request.
    ///
    /// Publishes any error before the release decrement. The source observes it
    /// after an acquire fence establishes batch completion.
    pub(crate) fn complete(&self, result: Result<(), FenceError>) {
        use core::sync::atomic::Ordering;
        if let Err(error) = result {
            self.0.completion_error.lock().get_or_insert(error);
        }
        self.0.wait_sync_count.fetch_sub(1, Ordering::Release);
    }
}
