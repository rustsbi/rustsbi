use rustsbi::{HartMask, MachineInfo, RustSBI};
use sbi_spec::binary::SbiRet;

#[derive(RustSBI)]
struct MySBI {
    fence: MyFence,
    info: MyMachineInfo,
}

struct MyFence;

impl rustsbi::Fence for MyFence {
    fn remote_fence_i(&self, _: HartMask) -> SbiRet {
        println!("remote fence i called");
        SbiRet::success(0)
    }

    fn remote_sfence_vma(&self, _: HartMask, _: usize, _: usize) -> SbiRet {
        todo!()
    }

    fn remote_sfence_vma_asid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet {
        todo!()
    }
}

struct MyMachineInfo;

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

fn main() {
    let sbi = MySBI {
        fence: MyFence,
        info: MyMachineInfo,
    };
    sbi.handle_ecall(sbi_spec::rfnc::EID_RFNC, 0, [0; 6]);
    let sbi_impl_id = sbi.handle_ecall(0x10, 0x1, [0; 6]);
    println!("SBI implementation ID: {:x?}", sbi_impl_id.value);
}
