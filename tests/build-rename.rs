use rustsbi::RustSBI;
use sbi_spec::{
    binary::{HartMask, Physical, SbiRet},
    dbcn::EID_DBCN,
    rfnc::EID_RFNC,
    spi::EID_SPI,
    time::EID_TIME,
};
use std::cell::RefCell;

#[derive(RustSBI)]
struct AssignOne {
    console: DummyConsole,
    #[rustsbi(fence)]
    some_other_name: DummyFence,
    info: DummyEnvInfo,
}

#[derive(RustSBI)]
struct AssignMultiple {
    #[rustsbi(ipi, timer)]
    clint: MockClint,
    info: DummyEnvInfo,
}

#[test]
fn rustsbi_assign_one() {
    let sbi = AssignOne {
        console: DummyConsole,
        some_other_name: DummyFence,
        info: DummyEnvInfo,
    };
    // Both extension return correct values.
    assert_eq!(sbi.handle_ecall(EID_DBCN, 0x0, [0; 6]), SbiRet::success(1));
    assert_eq!(sbi.handle_ecall(EID_DBCN, 0x1, [0; 6]), SbiRet::success(2));
    assert_eq!(sbi.handle_ecall(EID_DBCN, 0x2, [0; 6]), SbiRet::success(3));
    assert_eq!(sbi.handle_ecall(EID_RFNC, 0x0, [0; 6]), SbiRet::success(4));
    assert_eq!(sbi.handle_ecall(EID_RFNC, 0x1, [0; 6]), SbiRet::success(5));
    assert_eq!(sbi.handle_ecall(EID_RFNC, 0x2, [0; 6]), SbiRet::success(6));
    // Both extension exists.
    assert_ne!(
        sbi.handle_ecall(0x10, 0x3, [EID_DBCN, 0, 0, 0, 0, 0]),
        SbiRet::success(0)
    );
    assert_ne!(
        sbi.handle_ecall(0x10, 0x3, [EID_RFNC, 0, 0, 0, 0, 0]),
        SbiRet::success(0)
    );
}

#[test]
fn rustsbi_assign_multiple() {
    let sbi = AssignMultiple {
        clint: MockClint {
            time: RefCell::new(0),
        },
        info: DummyEnvInfo,
    };
    assert_eq!(sbi.handle_ecall(EID_SPI, 0x0, [0; 6]), SbiRet::success(10));
    #[cfg(target_pointer_width = "64")]
    sbi.handle_ecall(EID_TIME, 0x0, [0x1122334455667788, 0, 0, 0, 0, 0]);
    #[cfg(target_pointer_width = "32")]
    sbi.handle_ecall(EID_TIME, 0x0, [0x11223344, 0x55667788, 0, 0, 0, 0]);
    assert_eq!(sbi.clint.time.take(), 0x1122334455667788);
    assert_ne!(
        sbi.handle_ecall(0x10, 0x3, [EID_SPI, 0, 0, 0, 0, 0]),
        SbiRet::success(0)
    );
    assert_ne!(
        sbi.handle_ecall(0x10, 0x3, [EID_TIME, 0, 0, 0, 0, 0]),
        SbiRet::success(0)
    );
}

struct DummyConsole;

impl rustsbi::Console for DummyConsole {
    fn write(&self, _: Physical<&[u8]>) -> SbiRet {
        SbiRet::success(1)
    }

    fn read(&self, _: Physical<&mut [u8]>) -> SbiRet {
        SbiRet::success(2)
    }

    fn write_byte(&self, _: u8) -> SbiRet {
        SbiRet::success(3)
    }
}

struct DummyFence;

impl rustsbi::Fence for DummyFence {
    fn remote_fence_i(&self, _: HartMask) -> SbiRet {
        SbiRet::success(4)
    }

    fn remote_sfence_vma(&self, _: HartMask, _: usize, _: usize) -> SbiRet {
        SbiRet::success(5)
    }

    fn remote_sfence_vma_asid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet {
        SbiRet::success(6)
    }
}

struct DummyEnvInfo;

impl rustsbi::EnvInfo for DummyEnvInfo {
    fn mvendorid(&self) -> usize {
        unimplemented!()
    }

    fn marchid(&self) -> usize {
        unimplemented!()
    }

    fn mimpid(&self) -> usize {
        unimplemented!()
    }
}

struct RealEnvInfo;

impl rustsbi::EnvInfo for RealEnvInfo {
    fn mvendorid(&self) -> usize {
        7
    }

    fn marchid(&self) -> usize {
        8
    }

    fn mimpid(&self) -> usize {
        9
    }
}

struct MockClint {
    time: RefCell<u64>,
}

impl rustsbi::Timer for MockClint {
    fn set_timer(&self, stime_value: u64) {
        self.time.replace(stime_value);
    }
}

impl rustsbi::Ipi for MockClint {
    fn send_ipi(&self, _: HartMask) -> SbiRet {
        SbiRet::success(10)
    }
}
