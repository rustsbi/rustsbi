//! Fixed private trap-stack storage for every admitted hart.

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::config::HART_CAPACITY;
use crate::config::TRAP_STACK_SIZE;

use super::frame;

pub(crate) const ENTRY_SCRATCH_WORDS: usize = 5;
pub(crate) const ENTRY_SCRATCH_SIZE: usize = ENTRY_SCRATCH_WORDS * core::mem::size_of::<usize>();

#[repr(C, align(16))]
struct TrapStack(UnsafeCell<[MaybeUninit<u8>; TRAP_STACK_SIZE]>);

// SAFETY: each admitted dense hart index selects one disjoint stack. Rust
// never creates a slice over this storage; trap entry owns its fixed regions.
unsafe impl Sync for TrapStack {}

impl TrapStack {
    const fn new() -> Self {
        Self(UnsafeCell::new([MaybeUninit::uninit(); TRAP_STACK_SIZE]))
    }

    fn bottom(&self) -> usize {
        self.0.get().cast::<u8>() as usize
    }

    fn top(&self) -> usize {
        self.bottom()
            .checked_add(TRAP_STACK_SIZE)
            .expect("static trap-stack bounds fit usize")
    }
}

#[used]
#[unsafe(link_section = ".bss.trap_stack")]
static TRAP_STACKS: [TrapStack; HART_CAPACITY] = [const { TrapStack::new() }; HART_CAPACITY];

// Zero means that trap stacks have not been admitted. A successful boot
// publishes the complete dense prefix once, before any vector is activated.
static ADMITTED_HARTS: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StackError {
    AlreadyAdmitted,
    InvalidIndex,
    InvalidLayout,
}

pub(crate) fn admit(hart_count: usize) -> Result<(), StackError> {
    if hart_count == 0 || hart_count > HART_CAPACITY || !layout_is_valid() {
        return Err(StackError::InvalidLayout);
    }
    ADMITTED_HARTS
        .compare_exchange(0, hart_count, Ordering::Release, Ordering::Relaxed)
        .map(|_| ())
        .map_err(|_| StackError::AlreadyAdmitted)
}

pub(crate) fn top(index: usize) -> Result<usize, StackError> {
    let admitted = ADMITTED_HARTS.load(Ordering::Acquire);
    if index >= admitted {
        return Err(StackError::InvalidIndex);
    }
    Ok(TRAP_STACKS[index].top())
}

pub(crate) fn index_for_top(top: usize) -> Option<usize> {
    let admitted = ADMITTED_HARTS.load(Ordering::Acquire);
    TRAP_STACKS[..admitted]
        .iter()
        .position(|stack| stack.top() == top)
}

pub(crate) fn primary_frame(top: usize) -> Option<usize> {
    let index = index_for_top(top)?;
    let stack = &TRAP_STACKS[index];
    let unaligned = top
        .checked_sub(ENTRY_SCRATCH_SIZE)?
        .checked_sub(frame::FRAME_SIZE)?;
    let frame = unaligned & !(frame::FRAME_ALIGN - 1);
    (frame >= stack.bottom()
        && frame.is_multiple_of(frame::FRAME_ALIGN)
        && frame + frame::FRAME_SIZE + ENTRY_SCRATCH_SIZE <= top)
        .then_some(frame)
}

pub(crate) const fn layout_is_valid() -> bool {
    TRAP_STACK_SIZE.is_multiple_of(frame::FRAME_ALIGN)
        && frame::FRAME_SIZE.is_multiple_of(frame::FRAME_ALIGN)
        && TRAP_STACK_SIZE >= frame::FRAME_SIZE * 2 + ENTRY_SCRATCH_SIZE + frame::FRAME_ALIGN
}

pub(crate) const STACK_SIZE: usize = TRAP_STACK_SIZE;
pub(crate) const PRIMARY_FRAME_OFFSET: usize =
    (ENTRY_SCRATCH_SIZE + frame::FRAME_SIZE + frame::FRAME_ALIGN - 1) & !(frame::FRAME_ALIGN - 1);

const _: () = assert!(layout_is_valid());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_and_emergency_regions_are_disjoint() {
        let bottom = 0x1000usize;
        let top = bottom + TRAP_STACK_SIZE;
        let emergency_end = bottom + frame::FRAME_SIZE;
        let primary = top - PRIMARY_FRAME_OFFSET;
        assert!(emergency_end <= primary);
        assert!(primary.is_multiple_of(frame::FRAME_ALIGN));
    }
}
