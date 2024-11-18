use crate::sbi::extensions::HartExtensions;
use crate::sbi::hsm::HsmCell;
use crate::sbi::rfence::RFenceCell;
use core::ptr::NonNull;
use core::sync::atomic::AtomicU8;
use fast_trap::FlowContext;
use riscv::register::mstatus;

/// Context for managing hart (hardware thread) state and operations.
pub(crate) struct HartContext {
    /// Trap context for handling exceptions and interrupts.
    trap: FlowContext,
    /// Hart state management cell containing next stage boot info.
    pub hsm: HsmCell<NextStage>,
    /// Remote fence synchronization cell.
    pub rfence: RFenceCell,
    /// Type of inter-processor interrupt pending.
    pub ipi_type: AtomicU8,
    /// Supported hart extensions.
    pub extensions: HartExtensions,
}

impl HartContext {
    /// Initialize the hart context by creating new HSM and RFence cells
    #[inline]
    pub fn init(&mut self) {
        self.hsm = HsmCell::new();
        self.rfence = RFenceCell::new();
    }

    /// Get a non-null pointer to the trap context.
    #[inline]
    pub fn context_ptr(&mut self) -> NonNull<FlowContext> {
        unsafe { NonNull::new_unchecked(&mut self.trap) }
    }
}

/// Information needed to boot into the next execution stage.
#[derive(Debug)]
pub struct NextStage {
    /// Starting address to jump to.
    pub start_addr: usize,
    /// Opaque value passed to next stage.
    pub opaque: usize,
    /// Privilege mode for next stage.
    pub next_mode: mstatus::MPP,
}
