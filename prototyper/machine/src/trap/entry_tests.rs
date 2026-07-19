use super::*;
use crate::Trap;

struct Handler;

impl TrapHandler for Handler {
    fn handle(&self, _trap: Trap<'_>) -> ! {
        panic!("host layout test never dispatches")
    }
}

static HANDLER: Handler = Handler;

#[test]
fn hart_trap_state_initialization_is_transactional_and_one_time() {
    let stack = TrapStack::new();
    let state = HartTrapState::new();
    state.initialize(&stack, &HANDLER, 0, None, None).unwrap();
    assert!(state.is_ready());
    assert!(!state.has_hypervisor_metadata());
    state.enable_hypervisor_metadata();
    assert!(state.has_hypervisor_metadata());
    assert_eq!(
        state.initialize(&stack, &HANDLER, 0, None, None),
        Err(TrapStateError::AlreadyInitialized)
    );
}

#[test]
fn hart_trap_state_offsets_are_word_aligned() {
    for offset in [
        STACK_BOTTOM_OFFSET,
        STACK_TOP_OFFSET,
        CURRENT_FRAME_OFFSET,
        DEPTH_OFFSET,
        FLAGS_OFFSET,
        SAVED_SP_OFFSET,
        SAVED_T0_OFFSET,
        SAVED_T1_OFFSET,
        SAVED_T2_OFFSET,
        SAVED_T3_OFFSET,
    ] {
        assert_eq!(offset % core::mem::size_of::<usize>(), 0);
    }
}
