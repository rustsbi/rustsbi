//! Pending remote fences and completion acknowledgements.
//!
//! A source hart runs one synchronous SBI batch at a time and waits for all
//! acknowledgements and drains its error slot before issuing another batch.
//! The target iterator yields each hart at most once per batch, so a source
//! identifies at most one outstanding entry in any target queue. Dequeue and
//! cancellation serialize under the queue lock; exactly one path owns the ack.
//! Incoming requests serviced while waiting complete against their own source
//! hart's tracker and do not join the waiting batch.
//!
//! The sender increments pending before publishing under the queue lock. A
//! receiver records an error under the source tracker lock before its Release
//! decrement. Pending updates are atomic RMWs; the relaxed load that observes
//! the final zero followed by an Acquire fence synchronizes with the Release
//! decrements in that RMW release sequence. The tracker mutex protects the
//! later error-slot read as well.

#![forbid(unsafe_code)]

use alloc::collections::VecDeque;
use core::sync::atomic::AtomicU32;
use runtime::hart::{HartId, PerHart};
use runtime::rfence::FenceRequest;
use runtime::{Error, Result};
use spin::{Mutex, Once};

/// Capacity of the per-hart fence operation queue.
const QUEUE_CAP: usize = 16;

/// Requests sent to this hart, kept separate from outgoing completions.
// Alignment reduces false sharing; it does not guarantee cache-line isolation.
#[repr(align(128))]
struct RequestQueue {
    entries: Mutex<VecDeque<(FenceRequest, HartId)>>,
}

impl RequestQueue {
    fn new() -> Self {
        Self {
            entries: Mutex::new(VecDeque::new()),
        }
    }

    fn try_push(&self, item: (FenceRequest, HartId)) -> bool {
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

    fn pop(&self) -> Option<(FenceRequest, HartId)> {
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
    error: Mutex<Option<Error>>,
}

impl CompletionTracker {
    fn new() -> Self {
        Self {
            pending: AtomicU32::new(0),
            error: Mutex::new(None),
        }
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

    fn sub(&self) {
        use core::sync::atomic::Ordering;
        self.pending.fetch_sub(1, Ordering::Release);
    }

    fn complete(&self, result: Result<()>) {
        if let Err(error) = result {
            self.error.lock().get_or_insert(error);
        }
        self.sub();
    }

    fn take_error(&self) -> Option<Error> {
        assert!(self.is_sync(), "fence batch still has pending requests");
        self.error.lock().take()
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
    /// Checks if all synchronization operations are complete.
    pub fn is_sync(&self) -> bool {
        self.0.completions.is_sync()
    }

    /// Increments the synchronization counter.
    pub fn add(&self) {
        self.0.completions.add();
    }

    /// Takes the first execution error after all acknowledgements arrive.
    pub fn take_error(&self) -> Option<Error> {
        self.0.completions.take_error()
    }

    /// Returns the next fence operation from the queue, if any.
    pub fn get(&self) -> Option<(FenceRequest, HartId)> {
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
    pub(crate) fn try_push(&self, item: (FenceRequest, HartId)) -> bool {
        self.0.queue.try_push(item)
    }

    /// Decrements the synchronization counter.
    pub fn sub(&self) {
        self.0.completions.sub();
    }

    /// Records a remote execution error before publishing completion.
    pub fn complete(&self, result: Result<()>) {
        self.0.completions.complete(result);
    }
}
