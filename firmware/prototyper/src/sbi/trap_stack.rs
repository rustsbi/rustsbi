//! Per-hart trap stacks and the cross-hart cell primitives stored in each
//! slot. The stack array is externally-addressed raw memory (the naked trap
//! entry reaches it by symbol), so slots are `UnsafeCell` storage and all
//! Rust access goes through the safe per-hart accessors below.

use crate::cfg::{NUM_HART_MAX, STACK_SIZE_PER_HART};
use crate::riscv::current_hartid;
use crate::sbi::hart_context::{HartContext, HartLocal, NextStage};
use crate::sbi::rfence::RFenceContext;
use crate::sbi::trap::fast_handler;
use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::mem::{MaybeUninit, forget};
use core::ptr::{addr_of, addr_of_mut};
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use fast_trap::FreeTrapStack;
use rustsbi::spec::hsm::hart_state;
use spin::Mutex;

/// Raw stack slot for each hart.
///
/// Memory layout:
/// - Bottom: HartContext (trap frame + hart-local state).
/// - Middle: Stack space for the hart.
/// - Top: Trap handling space.
#[repr(C, align(128))]
struct HartStack(UnsafeCell<[u8; STACK_SIZE_PER_HART]>);

// SAFETY: `HartStack` is raw storage addressed by hart id: the naked entry
// (`locate`) reaches it via `sym ROOT_STACK` and the trap framework via the
// frame pointer installed by `load_as_stack`. Rust references are only
// formed for the caller's own slot inside `with_current`/`with_hart`, or as
// shared references through `hart_local`; cross-hart sharing goes through
// the atomics and cells stored in `HartLocal`.
unsafe impl Sync for HartStack {}

/// Root stack array for all harts, placed in BSS stack section.
#[used]
#[unsafe(link_section = ".bss.stack")]
static ROOT_STACK: [HartStack; NUM_HART_MAX] = [const { HartStack::zero() }; NUM_HART_MAX];

// Make sure stack address can be aligned.
const _: () = assert!(STACK_SIZE_PER_HART.is_multiple_of(core::mem::align_of::<HartStack>()));

/// Returns the raw slot of `hart_id`, or `None` when out of range.
#[inline]
fn slot(hart_id: usize) -> Option<&'static HartStack> {
    ROOT_STACK.get(hart_id)
}

/// Forms a shared reference to the hart-local state behind a raw slot.
#[inline]
fn local_of(slot: &'static HartStack) -> &'static HartLocal {
    // SAFETY: the slot memory is bounds-checked and static; the resulting
    // shared reference is only used for atomics and interior-mutable cells,
    // matching the pre-split `hart_context()` projection.
    unsafe { &*addr_of!((*slot.0.get().cast::<HartContext>()).local) }
}

/// Locates and initializes stack for each hart.
///
/// This is a naked function that sets up the stack pointer based on hart ID.
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn locate() {
    core::arch::naked_asm!(
        "   la   sp, {stack}            // Load stack base address
            li   t0, {per_hart_stack_size} // Load stack size per hart
            csrr t1, mhartid            // Get current hart ID
            addi t1, t1,  1             // Add 1 to hart ID
         1: add  sp, sp, t0             // Calculate stack pointer
            addi t1, t1, -1             // Decrement counter
            bnez t1, 1b                 // Loop if not zero
            call t1, {move_stack}       // Call stack reuse function
            ret                         // Return
        ",
        per_hart_stack_size = const STACK_SIZE_PER_HART,
        stack               =   sym ROOT_STACK,
        move_stack          =   sym fast_trap::reuse_stack_for_trap,
    )
}

/// Prepares trap stack for current hart
pub(crate) fn prepare_for_trap() {
    // SAFETY: the current hart id always indexes `ROOT_STACK`.
    let slot: &'static HartStack = unsafe { ROOT_STACK.get_unchecked(current_hartid()) };
    HartStack::load_as_stack(slot);
}

/// Runs `f` with exclusive access to the current hart's hart-local state.
///
/// SAFETY (mechanism contract): each slot is owned by exactly one hart, so
/// borrowing that hart's `HartLocal` is exclusive while `f` runs — M-mode
/// trap entry clears MIE and trap/interrupt handlers never call this, so
/// no second borrow can go live. The [`TrapFrame`] part of the slot is
/// unreachable through the given reference.
///
/// [`TrapFrame`]: crate::sbi::hart_context::TrapFrame
pub fn with_current<F, R>(f: F) -> R
where
    F: FnOnce(&mut HartLocal) -> R,
{
    with_hart(current_hartid(), f)
}

/// Runs `f` with exclusive access to an arbitrary hart's hart-local state.
///
/// SAFETY (mechanism contract): the caller must guarantee the target hart
/// does not concurrently touch its own slot while `f` runs (parked in
/// firmware entry, or not yet released). Used by `with_current` and by the
/// boot-time feature seeding in `features::extension_detection`.
pub fn with_hart<F, R>(hart_id: usize, f: F) -> R
where
    F: FnOnce(&mut HartLocal) -> R,
{
    let slot = slot(hart_id).expect("hart id within ROOT_STACK bounds");
    // SAFETY: exclusive to this slot per the contract above; the memory is
    // bounds-checked, static, and disjoint from the trap frame at offset 0.
    let local = unsafe { &mut *addr_of_mut!((*slot.0.get().cast::<HartContext>()).local) };
    f(local)
}

/// Shared projection of any hart's hart-local state.
pub fn hart_local(hart_id: usize) -> &'static HartLocal {
    local_of(slot(hart_id).expect("hart id within ROOT_STACK bounds"))
}

/// Size of the FIFO buffer.
const FIFO_SIZE: usize = 16;

#[derive(Debug)]
pub enum FifoError {
    Empty,
    Full,
}

/// A fixed-size FIFO (First In First Out) queue implementation.
pub struct Fifo<T: Copy + Clone> {
    data: [MaybeUninit<T>; FIFO_SIZE],
    head: usize,
    tail: usize,
    count: usize,
}

impl<T: Copy + Clone> Fifo<T> {
    #[inline]
    pub const fn new() -> Self {
        // Initialize array with uninitialized values
        let data = [MaybeUninit::uninit(); FIFO_SIZE];
        Self {
            data,
            head: 0,
            tail: 0,
            count: 0,
        }
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.count == FIFO_SIZE
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn push(&mut self, element: T) -> Result<(), FifoError> {
        if self.is_full() {
            return Err(FifoError::Full);
        }

        // Write element and update tail position
        self.data[self.tail].write(element);
        self.tail = (self.tail + 1) % FIFO_SIZE;
        self.count += 1;

        Ok(())
    }

    pub fn pop(&mut self) -> Result<T, FifoError> {
        if self.is_empty() {
            return Err(FifoError::Empty);
        }

        // unsafe: Take ownership of element at head
        let element = unsafe { self.data[self.head].assume_init_read() };

        // Update head position
        self.head = (self.head + 1) % FIFO_SIZE;
        self.count -= 1;

        Ok(element)
    }
}

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

/// Cell for managing remote fence operations between harts.
pub(crate) struct RFenceCell {
    // Queue of fence operations with source hart ID
    queue: Mutex<Fifo<(RFenceContext, usize)>>,
    // Counter for tracking pending synchronization operations
    wait_sync_count: AtomicU32,
}

impl RFenceCell {
    /// Creates a new RFenceCell with empty queue and zero sync count.
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(Fifo::new()),
            wait_sync_count: AtomicU32::new(0),
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
    /// The retry policy on [`FifoError::Full`] (drain via IPI) belongs to
    /// the rfence layer, which is why this returns the error instead of
    /// looping.
    pub fn try_push(&self, item: (RFenceContext, usize)) -> Result<(), FifoError> {
        self.queue.lock().push(item)
    }
}

// Mark RFenceCell as safe to share between threads
unsafe impl Sync for RFenceCell {}
unsafe impl Send for RFenceCell {}

/// View of RFenceCell for operations on the current hart.
pub struct LocalRFenceCell<'a>(&'a RFenceCell);

/// View of RFenceCell for operations from other harts.
pub struct RemoteRFenceCell<'a>(&'a RFenceCell);

#[allow(unused)]
impl LocalRFenceCell<'_> {
    /// Pushes a fence operation into the queue, or reports it full.
    #[inline]
    pub(crate) fn try_push(&self, item: (RFenceContext, usize)) -> Result<(), FifoError> {
        self.0.try_push(item)
    }

    /// Checks if all synchronization operations are complete.
    pub fn is_sync(&self) -> bool {
        self.0.wait_sync_count.load(Ordering::Relaxed) == 0
    }

    /// Increments the synchronization counter.
    pub fn add(&self) {
        self.0.wait_sync_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Checks if the operation queue is empty.
    pub fn is_empty(&self) -> bool {
        self.0.queue.lock().is_empty()
    }

    /// Gets the next fence operation from the queue.
    pub fn get(&self) -> Option<(RFenceContext, usize)> {
        self.0.queue.lock().pop().ok()
    }
}

#[allow(unused)]
impl RemoteRFenceCell<'_> {
    /// Pushes a fence operation into the queue, or reports it full.
    #[inline]
    pub(crate) fn try_push(&self, item: (RFenceContext, usize)) -> Result<(), FifoError> {
        self.0.try_push(item)
    }

    /// Decrements the synchronization counter.
    pub fn sub(&self) {
        self.0.wait_sync_count.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Gets the local HSM cell for the current hart.
pub fn local_hsm() -> LocalHsmCell<'static, NextStage> {
    // SAFETY: the cell belongs to the current hart by construction.
    unsafe { hart_local(current_hartid()).hsm.local() }
}

/// Gets a remote view of any hart's HSM cell.
pub fn remote_hsm(hart_id: usize) -> Option<RemoteHsmCell<'static, NextStage>> {
    slot(hart_id).map(local_of).map(|local| local.hsm.remote())
}

/// Gets the local fence context for the current hart.
pub fn local_rfence() -> Option<LocalRFenceCell<'static>> {
    slot(current_hartid())
        .map(local_of)
        .map(|local| local.rfence.local())
}

/// Gets the remote fence context for a specific hart.
pub fn remote_rfence(hart_id: usize) -> Option<RemoteRFenceCell<'static>> {
    slot(hart_id)
        .map(local_of)
        .map(|local| local.rfence.remote())
}

/// Resets the hart-local bookkeeping (ipi type, fence cell, pmu state) of
/// `hart_id`; the trap frame is untouched, as in the pre-split reset.
///
/// SAFETY: the target hart is being resumed via the HSM path, and this
/// reset runs before that hart next touches its `HartLocal` — its wake
/// path is `boot()`, which does not.
pub fn reset_hart(hart_id: usize) {
    with_hart(hart_id, HartLocal::reset);
}

impl HartStack {
    /// All-zero slot, usable as an array repeat operand (const fn form;
    /// a `const` item would carry interior mutability).
    const fn zero() -> Self {
        Self(UnsafeCell::new([0; STACK_SIZE_PER_HART]))
    }

    /// Initializes stack for trap handling.
    /// - Sets up hart context.
    /// - Creates and loads FreeTrapStack with the stack range.
    fn load_as_stack(slot: &'static Self) {
        let context = slot.0.get().cast::<HartContext>();
        // SAFETY: this hart owns `slot`, and the trap entry starts using the
        // installed frame pointer only once `FreeTrapStack::load` runs below.
        let context_ptr = unsafe { (*addr_of_mut!((*context).frame)).context_ptr() };
        unsafe { (*addr_of_mut!((*context).local)).init() };

        // Get stack memory range.
        let range = unsafe { (*slot.0.get()).as_ptr_range() };

        // Create and load trap stack, forgetting it to avoid drop
        forget(
            FreeTrapStack::new(
                range.start as usize..range.end as usize,
                |_| {}, // Empty callback
                context_ptr,
                fast_handler,
            )
            .unwrap()
            .load(),
        );
    }
}
