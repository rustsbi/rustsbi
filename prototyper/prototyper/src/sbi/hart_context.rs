use crate::sbi::features::HartFeatures;
use crate::sbi::features::PrivilegedVersion;
use crate::sbi::hsm::HsmCell;
use crate::sbi::rfence::RFenceCell;
use core::ptr::NonNull;
use core::sync::atomic::AtomicU8;
use core::sync::atomic::Ordering;
use fast_trap::FlowContext;
use riscv::register::mstatus;

use super::pmu::PmuState;

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
    /// Supported hart features.
    pub features: HartFeatures,
    /// PMU State
    pub pmu_state: PmuState,
}

// Make sure HartContext is aligned.
//
// HartContext will always at the end of Stack, so we should make sure
// STACK_SIZE_PER_HART is a multiple of b.
use crate::cfg::STACK_SIZE_PER_HART;
const _: () = assert!(STACK_SIZE_PER_HART % core::mem::align_of::<HartContext>() == 0);

impl HartContext {
    /// Initialize the hart context by creating new HSM and RFence cells
    #[inline]
    pub fn init(&mut self) {
        self.hsm = HsmCell::new();
        self.rfence = RFenceCell::new();
        self.pmu_state = PmuState::new();
    }

    /// Get a non-null pointer to the trap context.
    #[inline]
    pub fn context_ptr(&mut self) -> NonNull<FlowContext> {
        unsafe { NonNull::new_unchecked(&mut self.trap) }
    }

    #[inline]
    pub fn reset(&mut self) {
        self.ipi_reset();
        self.rfence_reset();
        self.pmu_state_reset();
    }

    #[inline]
    fn rfence_reset(&mut self) {
        self.rfence = RFenceCell::new();
    }

    #[inline]
    fn ipi_reset(&mut self) {
        self.ipi_type.store(0, Ordering::Relaxed);
    }

    #[inline]
    fn pmu_state_reset(&mut self) {
        // stop all hardware pmu event
        let hart_priv_version = self.features.privileged_version();
        if hart_priv_version >= PrivilegedVersion::Version1_11 {
            unsafe {
                core::arch::asm!("csrw mcountinhibit, {}", in(reg) !0b10);
            }
        }
        // reset hart pmu state
        self.pmu_state = PmuState::new();
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
