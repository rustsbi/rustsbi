use core::ptr::NonNull;
use fast_trap::FlowContext;
use riscv::register::mstatus;

use crate::sbi::hsm::HsmCell;
use crate::sbi::rfence::RFenceCell;
use core::sync::atomic::AtomicU8;

pub(crate) struct HartContext {
    /// trap context
    trap: FlowContext,
    pub hsm: HsmCell<NextStage>,
    pub rfence: RFenceCell,
    pub ipi_type: AtomicU8,
}

impl HartContext {
    #[inline]
    pub fn init(&mut self) {
        self.hsm = HsmCell::new();
        self.rfence = RFenceCell::new();
    }

    #[inline]
    pub fn context_ptr(&mut self) -> NonNull<FlowContext> {
        unsafe { NonNull::new_unchecked(&mut self.trap) }
    }
}

/// Next Stage boot info
#[derive(Debug)]
pub struct NextStage {
    pub start_addr: usize,
    pub opaque: usize,
    pub next_mode: mstatus::MPP,
}
