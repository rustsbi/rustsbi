use core::ptr::NonNull;
use fast_trap::FlowContext;

use crate::hsm::HsmCell;
use crate::rfence::RFenceCell;
use crate::NextStage;


/// 硬件线程上下文。
pub(crate) struct HartContext {
    /// 陷入上下文。
    trap: FlowContext,
    pub hsm: HsmCell<NextStage>,
    pub rfence: RFenceCell
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
