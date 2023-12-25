// Mock implementaion module. Actual SBI implementaion should implement
// those SBI extensions with machine environment specific hardware features.

use rustsbi::EnvInfo;
use sbi_spec::binary::{HartMask, SbiRet};

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

    fn remote_hfence_gvma_vmid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet {
        todo!()
    }

    fn remote_hfence_gvma(&self, _: HartMask, _: usize, _: usize) -> SbiRet {
        todo!()
    }

    fn remote_hfence_vvma_asid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet {
        todo!()
    }

    fn remote_hfence_vvma(&self, _: HartMask, _: usize, _: usize) -> SbiRet {
        todo!()
    }
}

pub struct MyEnvInfo;

impl EnvInfo for MyEnvInfo {
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
