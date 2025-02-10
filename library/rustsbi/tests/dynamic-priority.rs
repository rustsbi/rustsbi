use rustsbi::{HartMask, RustSBI, SbiRet};
use sbi_spec::{rfnc::EID_RFNC, spi::EID_SPI, time::EID_TIME};

#[derive(RustSBI)]
#[rustsbi(dynamic)]
struct MultipleFences {
    #[rustsbi(fence)]
    fence_one: Option<FenceOne>,
    #[rustsbi(fence)]
    fence_two: Option<FenceTwo>,
    env_info: DummyEnvInfo,
}

#[test]
fn priority_multiple_fences() {
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

#[derive(RustSBI)]
#[rustsbi(dynamic)]
struct AssignMultiple {
    #[rustsbi(timer)]
    aclint_1: Option<AclintOne>,
    #[rustsbi(ipi, timer)]
    aclint_2: Option<AclintTwo>,
    #[rustsbi(ipi)]
    aclint_3: Option<AclintThree>,
    env_info: DummyEnvInfo,
}

#[test]
fn priority_assign_multiple() {
    let sbi = AssignMultiple {
        aclint_1: Some(AclintOne::default()),
        aclint_2: Some(AclintTwo::default()),
        aclint_3: Some(AclintThree::default()),
        env_info: DummyEnvInfo,
    };
    let sbi_ret = sbi.handle_ecall(EID_TIME, 0x0, [1, 0, 0, 0, 0, 0]);
    assert_eq!(sbi_ret, SbiRet::success(0));
    assert_eq!(sbi.aclint_1.as_ref().unwrap().stimer_value(), 1);
    assert_eq!(sbi.aclint_2.as_ref().unwrap().stimer_value(), 0);
    let sbi_ret = sbi.handle_ecall(EID_SPI, 0x0, [0; 6]);
    assert_eq!(sbi_ret, SbiRet::success(7));

    let sbi = AssignMultiple {
        aclint_1: None,
        aclint_2: Some(AclintTwo::default()),
        aclint_3: Some(AclintThree::default()),
        env_info: DummyEnvInfo,
    };
    let sbi_ret = sbi.handle_ecall(EID_TIME, 0x0, [2, 0, 0, 0, 0, 0]);
    assert_eq!(sbi_ret, SbiRet::success(0));
    assert_eq!(sbi.aclint_2.as_ref().unwrap().stimer_value(), 2);
    let sbi_ret = sbi.handle_ecall(EID_SPI, 0x0, [0; 6]);
    assert_eq!(sbi_ret, SbiRet::success(7));

    let sbi = AssignMultiple {
        aclint_1: None,
        aclint_2: None,
        aclint_3: Some(AclintThree::default()),
        env_info: DummyEnvInfo,
    };
    let sbi_ret = sbi.handle_ecall(EID_TIME, 0x0, [3, 0, 0, 0, 0, 0]);
    assert_eq!(sbi_ret, SbiRet::not_supported());
    let sbi_ret = sbi.handle_ecall(EID_SPI, 0x0, [0; 6]);
    assert_eq!(sbi_ret, SbiRet::success(8));

    let sbi = AssignMultiple {
        aclint_1: None,
        aclint_2: None,
        aclint_3: None,
        env_info: DummyEnvInfo,
    };
    let sbi_ret = sbi.handle_ecall(EID_TIME, 0x0, [4, 0, 0, 0, 0, 0]);
    assert_eq!(sbi_ret, SbiRet::not_supported());
    let sbi_ret = sbi.handle_ecall(EID_SPI, 0x0, [0; 6]);
    assert_eq!(sbi_ret, SbiRet::not_supported());
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

#[derive(Default)]
struct AclintOne {
    inner: core::cell::RefCell<u64>,
}

impl AclintOne {
    fn stimer_value(&self) -> u64 {
        *self.inner.borrow()
    }
}

impl rustsbi::Timer for AclintOne {
    fn set_timer(&self, stime_value: u64) {
        let _ = self.inner.replace(stime_value);
    }
}

#[derive(Default)]
struct AclintTwo {
    inner: core::cell::RefCell<u64>,
}

impl AclintTwo {
    fn stimer_value(&self) -> u64 {
        *self.inner.borrow()
    }
}

impl rustsbi::Timer for AclintTwo {
    fn set_timer(&self, stime_value: u64) {
        let _ = self.inner.replace(stime_value);
    }
}

impl rustsbi::Ipi for AclintTwo {
    fn send_ipi(&self, _: HartMask) -> SbiRet {
        SbiRet::success(7)
    }
}

#[derive(Default)]
struct AclintThree;

impl rustsbi::Ipi for AclintThree {
    fn send_ipi(&self, _: HartMask) -> SbiRet {
        SbiRet::success(8)
    }
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
