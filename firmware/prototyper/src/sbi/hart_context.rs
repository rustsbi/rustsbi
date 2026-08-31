use crate::sbi::features::HartFeatures;
use crate::sbi::features::PrivilegedVersion;
use core::ptr::NonNull;
use core::sync::atomic::AtomicU8;
use core::sync::atomic::Ordering;
use fast_trap::FlowContext;
use riscv::register::mstatus;

use super::fwft::FwftState;
use super::pmu::PmuState;
use super::trap_stack::{HsmCell, RFenceCell};

/// Raw per-hart context sitting at the bottom of each trap stack slot.
///
/// This composition keeps the pre-split `HartContext` layout byte-identical:
/// `repr(C)` with `frame` first leaves the trap [`FlowContext`] at offset 0,
/// where the naked entry and the trap framework address it, with the
/// sbi-visible [`HartLocal`] state directly behind it.
#[repr(C)]
pub(crate) struct HartContext {
    /// Trap frame; the only part the trap path touches.
    pub(crate) frame: TrapFrame,
    /// Hart-local state consumed by the sbi extension layer.
    pub(crate) local: HartLocal,
}

/// The part of a hart's slot owned by the trap path.
pub(crate) struct TrapFrame {
    /// Trap context for handling exceptions and interrupts.
    pub(crate) trap: FlowContext,
}

impl TrapFrame {
    /// Get a non-null pointer to the trap context.
    #[inline]
    pub(crate) fn context_ptr(&mut self) -> NonNull<FlowContext> {
        unsafe { NonNull::new_unchecked(&mut self.trap) }
    }
}

/// Hart-local state, consumed by the sbi extension layer.
///
/// Reached only through the safe accessors of [`crate::sbi::trap_stack`]
/// (`with_current`/`hart_local` and the cell projections); it is kept in a
/// separate type from [`TrapFrame`] so an exclusive borrow here can never
/// alias the trap framework's view of the same stack slot.
pub struct HartLocal {
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
    /// Per-hart FWFT lock and feature-probe state.
    pub(crate) fwft_state: FwftState,
}

// HartContext sits at the bottom of each HartStack slot, so the slot size
// must be a multiple of its alignment; same for the two members.
use crate::cfg::STACK_SIZE_PER_HART;
const _: () = assert!(STACK_SIZE_PER_HART.is_multiple_of(core::mem::align_of::<HartContext>()));
const _: () = assert!(STACK_SIZE_PER_HART.is_multiple_of(core::mem::align_of::<TrapFrame>()));
const _: () = assert!(STACK_SIZE_PER_HART.is_multiple_of(core::mem::align_of::<HartLocal>()));

// Layout preservation across the TrapFrame/HartLocal split: `trap` was the
// first field of the pre-split `HartContext`, and `repr(C)` with `frame`
// first keeps it at offset 0.
const _: () = assert!(core::mem::offset_of!(HartContext, frame) == 0);

impl HartLocal {
    /// Initialize the hart-local state by creating new HSM and RFence cells
    #[inline]
    pub fn init(&mut self) {
        self.hsm = HsmCell::new();
        self.rfence = RFenceCell::new();
        self.pmu_state = PmuState::new();
        self.fwft_state = FwftState::new();
    }

    #[inline]
    pub fn reset(&mut self) {
        self.ipi_reset();
        self.rfence_reset();
        self.pmu_state_reset();
        self.fwft_state.reset();
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
                core::arch::asm!("csrw mcountinhibit, {}", in(reg) !0b111usize);
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
