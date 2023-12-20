// Mock implementaion module. Actual SBI implementaion should implement
// those SBI extensions with machine environment specific hardware features.

use rustsbi::{HartMask, MachineInfo};
use sbi_spec::binary::SbiRet;

pub struct MyFence;

impl rustsbi::Fence for MyFence {
    fn remote_fence_i(&self, _: HartMask) -> SbiRet {
        println!("MyFence remote_fence_i function is called!");
        SbiRet::success(0)
    }

    fn remote_sfence_vma(&self, _: HartMask, _: usize, _: usize) -> SbiRet {
        todo!()
    }

    fn remote_sfence_vma_asid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet {
        todo!()
    }
}

pub struct MyMachineInfo;

impl MachineInfo for MyMachineInfo {
    fn mvendorid(&self) -> usize {
        0x100
    }

    fn marchid(&self) -> usize {
        0x200
    }

    fn mimpid(&self) -> usize {
        0x300
    }
}
