use rustsbi::{HartMask, RustSBI};
use sbi_spec::binary::SbiRet;

#[derive(RustSBI)]
struct MySBI {
    fence: MyFence,
}

struct MyFence;

impl rustsbi::Fence for MyFence {
    fn remote_fence_i(&self, _: HartMask) -> SbiRet {
        println!("remote fence i");
        SbiRet::success(0)
    }

    fn remote_sfence_vma(&self, _: HartMask, _: usize, _: usize) -> SbiRet {
        todo!()
    }

    fn remote_sfence_vma_asid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet {
        todo!()
    }
}

fn main() {
    let sbi = MySBI { fence: MyFence };
    sbi.handle_ecall(sbi_spec::rfnc::EID_RFNC, 0, [0; 6]);
    let spec_version = sbi.handle_ecall(
        sbi_spec::base::EID_BASE,
        sbi_spec::base::GET_SBI_SPEC_VERSION,
        [0; 6],
    );
    println!("spec version: {:x?}", spec_version.value);
}
