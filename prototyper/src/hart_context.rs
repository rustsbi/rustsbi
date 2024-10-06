use core::ptr::NonNull;
use fast_trap::FlowContext;

use crate::hsm::HsmCell;
use crate::rfence::RFenceCell;
use crate::NextStage;
use core::sync::atomic::AtomicU8;

/// 硬件线程上下文。
pub(crate) struct HartContext {
    /// 陷入上下文。
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
