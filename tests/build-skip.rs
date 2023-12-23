use rustsbi::RustSBI;
use sbi_spec::{
    binary::{Physical, SbiRet},
    dbcn::EID_DBCN,
    rfnc::EID_RFNC,
};

#[derive(RustSBI)]
struct SkipExtension {
    console: DummyConsole,
    #[rustsbi(skip)]
    fence: DummyFence,
    info: DummyEnvInfo,
}

#[derive(RustSBI)]
struct SkipEnvInfo {
    console: DummyConsole,
    fence: DummyFence,
    #[rustsbi(skip)]
    info: DummyEnvInfo,
    env_info: RealEnvInfo,
}

#[test]
fn rustsbi_skip_extension() {
    let sbi = SkipExtension {
        console: DummyConsole,
        fence: DummyFence,
        info: DummyEnvInfo,
    };
    // 1. Skipped fields are neither used during RustSBI macro generation, ...
    assert_eq!(sbi.handle_ecall(EID_DBCN, 0x0, [0; 6]), SbiRet::success(1));
    assert_eq!(sbi.handle_ecall(EID_DBCN, 0x1, [0; 6]), SbiRet::success(2));
    assert_eq!(sbi.handle_ecall(EID_DBCN, 0x2, [0; 6]), SbiRet::success(3));
    assert_eq!(
        sbi.handle_ecall(EID_RFNC, 0x0, [0; 6]),
        SbiRet::not_supported()
    );
    assert_eq!(
        sbi.handle_ecall(EID_RFNC, 0x1, [0; 6]),
        SbiRet::not_supported()
    );
    assert_eq!(
        sbi.handle_ecall(EID_RFNC, 0x2, [0; 6]),
        SbiRet::not_supported()
    );
    // 2. ... nor do they appear in extension detection in Base extension.
    // note that it's `assert_ne` - the handle_ecall should not return a success detection with
    // value 0 indicating this feature is not supported, ...
    assert_ne!(
        sbi.handle_ecall(0x10, 0x3, [EID_DBCN, 0, 0, 0, 0, 0]),
        SbiRet::success(0)
    );
    // ... and the `assert_eq` here means extension detection detected successfully that this
    // extension is not supported in SBI implementation `SkipExtension`.
    assert_eq!(
        sbi.handle_ecall(0x10, 0x3, [EID_RFNC, 0, 0, 0, 0, 0]),
        SbiRet::success(0)
    );
    // Additionally, we illustrate here that the skipped fields may be used elsewhere.
    let _ = sbi.fence;
}

#[test]
fn rustsbi_skip_env_info() {
    let sbi = SkipEnvInfo {
        console: DummyConsole,
        fence: DummyFence,
        info: DummyEnvInfo,
        env_info: RealEnvInfo,
    };
    // The `env_info` instead of `info` field would be used by RustSBI macro; struct
    // `RealEnvInfo` would return 7, 8 and 9 for mvendorid, marchid and mimpid.
    assert_eq!(
        sbi.handle_ecall(0x10, 0x4, [EID_DBCN, 0, 0, 0, 0, 0]),
        SbiRet::success(7)
    );
    assert_eq!(
        sbi.handle_ecall(0x10, 0x5, [EID_DBCN, 0, 0, 0, 0, 0]),
        SbiRet::success(8)
    );
    assert_eq!(
        sbi.handle_ecall(0x10, 0x6, [EID_DBCN, 0, 0, 0, 0, 0]),
        SbiRet::success(9)
    );
    let _ = sbi.info;
}

// Return values of following trait impls are special values,
// they are used by software to detect if one implementation is used by RustSBI.

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
