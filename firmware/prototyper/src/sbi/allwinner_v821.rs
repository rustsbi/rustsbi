//! DT-selected V821 custom SBI extensions.

use crate::riscv::allwinner_v821;
use runtime::{
    memory::SupervisorMemory,
    rustsbi::{Extension, SbiRet},
};

pub(crate) struct Andes {
    memory: &'static SupervisorMemory,
}

impl Andes {
    pub(crate) fn new(memory: &'static SupervisorMemory) -> Option<Self> {
        allwinner_v821::available().then_some(Self { memory })
    }
}

impl Extension for Andes {
    #[inline]
    fn probe(&self) -> usize {
        1
    }

    fn handle(&self, fid: usize, args: [usize; 6]) -> SbiRet {
        allwinner_v821::handle(fid, args, self.memory)
    }
}

pub(crate) struct Awbase;

impl Awbase {
    pub(crate) fn new() -> Option<Self> {
        allwinner_v821::awbase_available().then_some(Self)
    }
}

impl Extension for Awbase {
    #[inline]
    fn probe(&self) -> usize {
        1
    }

    fn handle(&self, fid: usize, _: [usize; 6]) -> SbiRet {
        allwinner_v821::handle_awbase(fid)
    }
}
