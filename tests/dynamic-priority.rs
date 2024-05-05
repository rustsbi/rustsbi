use rustsbi::{RustSBI, SbiRet};
use sbi_spec::rfnc::EID_RFNC;

#[derive(RustSBI)]
#[rustsbi(dynamic)]
struct MultipleFences {
    #[rustsbi(fence)]
    fence_one: Option<FenceOne>,
    #[rustsbi(fence)]
    fence_two: Option<FenceTwo>,
    env_info: DummyEnvInfo,
}

struct FenceOne;

impl rustsbi::Fence for FenceOne {
    fn remote_fence_i(&self, _: rustsbi::HartMask) -> SbiRet {
        SbiRet::success(1)
    }

    fn remote_sfence_vma(&self, _: rustsbi::HartMask, _: usize, _: usize) -> SbiRet {
        SbiRet::success(2)
    }

    fn remote_sfence_vma_asid(&self, _: rustsbi::HartMask, _: usize, _: usize, _: usize) -> SbiRet {
        SbiRet::success(3)
    }
}

struct FenceTwo;

impl rustsbi::Fence for FenceTwo {
    fn remote_fence_i(&self, _: rustsbi::HartMask) -> SbiRet {
        SbiRet::success(4)
    }

    fn remote_sfence_vma(&self, _: rustsbi::HartMask, _: usize, _: usize) -> SbiRet {
        SbiRet::success(5)
    }

    fn remote_sfence_vma_asid(&self, _: rustsbi::HartMask, _: usize, _: usize, _: usize) -> SbiRet {
        SbiRet::success(6)
    }
}

#[test]
fn priority() {
    let sbi = MultipleFences {
        fence_one: Some(FenceOne),
        fence_two: None,
        env_info: DummyEnvInfo,
    };
    assert_eq!(sbi.handle_ecall(EID_RFNC, 0x0, [0; 6]), SbiRet::success(1));
    let sbi = MultipleFences {
        fence_one: None,
        fence_two: Some(FenceTwo),
        env_info: DummyEnvInfo,
    };
    assert_eq!(sbi.handle_ecall(EID_RFNC, 0x0, [0; 6]), SbiRet::success(4));
    let sbi = MultipleFences {
        fence_one: Some(FenceOne),
        fence_two: Some(FenceTwo),
        env_info: DummyEnvInfo,
    };
    assert_eq!(sbi.handle_ecall(EID_RFNC, 0x0, [0; 6]), SbiRet::success(1));
    let sbi = MultipleFences {
        fence_one: None,
        fence_two: None,
        env_info: DummyEnvInfo,
    };
    assert_eq!(
        sbi.handle_ecall(EID_RFNC, 0x0, [0; 6]),
        SbiRet::not_supported()
    );
}

struct DummyEnvInfo;

impl rustsbi::EnvInfo for DummyEnvInfo {
    fn mvendorid(&self) -> usize {
        36
    }

    fn marchid(&self) -> usize {
        37
    }

    fn mimpid(&self) -> usize {
        38
    }
}
