use rustsbi::RustSBI;
use sbi_spec::binary::SbiRet;

// These structs should pass Rust build.

#[derive(RustSBI)]
#[rustsbi(dynamic)]
struct WithGenerics<T: rustsbi::Timer> {
    reset: DummyReset,
    timer: T,
    info: DummyEnvInfo,
}

#[derive(RustSBI)]
#[rustsbi(dynamic)]
struct WithWhereClause<T>
where
    T: rustsbi::Timer,
{
    reset: DummyReset,
    timer: T,
    info: DummyEnvInfo,
}

#[derive(RustSBI)]
#[rustsbi(dynamic)]
struct WithConstantGenerics<const N: usize> {
    info: DummyEnvInfo,
    _dummy: [u8; N],
}

#[derive(RustSBI)]
#[rustsbi(dynamic)]
struct WithLifetime<'a> {
    info: &'a DummyEnvInfo,
}

#[derive(RustSBI)]
#[rustsbi(dynamic)]
struct WithEverythingCombined<'a, T: rustsbi::Timer, U, const N: usize>
where
    U: rustsbi::Reset,
{
    timer: T,
    reset: U,
    info: &'a DummyEnvInfo,
    _dummy: [u8; N],
}

#[test]
fn test_impl_id() {
    let sbi = WithGenerics {
        reset: DummyReset,
        timer: DummyTimer,
        info: DummyEnvInfo,
    };
    assert_eq!(sbi.handle_ecall(0x10, 0x1, [0; 6]).value, 4);
    let sbi = WithWhereClause {
        reset: DummyReset,
        timer: DummyTimer,
        info: DummyEnvInfo,
    };
    assert_eq!(sbi.handle_ecall(0x10, 0x1, [0; 6]).value, 4);
    let sbi = WithConstantGenerics {
        info: DummyEnvInfo,
        _dummy: [0; 100],
    };
    assert_eq!(sbi.handle_ecall(0x10, 0x1, [0; 6]).value, 4);
    let dummy_info = DummyEnvInfo;
    let sbi = WithLifetime { info: &dummy_info };
    assert_eq!(sbi.handle_ecall(0x10, 0x1, [0; 6]).value, 4);
    let sbi = WithEverythingCombined {
        timer: DummyTimer,
        reset: DummyReset,
        info: &dummy_info,
        _dummy: [0; 10],
    };
    assert_eq!(sbi.handle_ecall(0x10, 0x1, [0; 6]).value, 4);
}

struct DummyReset;

impl rustsbi::Reset for DummyReset {
    fn system_reset(&self, _: u32, _: u32) -> SbiRet {
        unimplemented!()
    }
}

struct DummyTimer;

impl rustsbi::Timer for DummyTimer {
    fn set_timer(&self, _: u64) {
        unimplemented!()
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
