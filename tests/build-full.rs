use rustsbi::RustSBI;
use sbi_spec::{
    binary::{HartMask, Physical, SbiRet, SharedPtr},
    nacl::shmem_size::NATIVE,
};

#[derive(RustSBI)]
struct FullyImplemented {
    console: DummyConsole,
    cppc: DummyCppc,
    hsm: DummyHsm,
    ipi: DummyIpi,
    nacl: DummyNacl,
    pmu: DummyPmu,
    reset: DummyReset,
    fence: DummyFence,
    sta: DummySta,
    susp: DummySusp,
    timer: DummyTimer,
    info: DummyEnvInfo,
}

#[derive(RustSBI)]
struct AlternateName {
    dbcn: DummyConsole,
    cppc: DummyCppc,
    hsm: DummyHsm,
    ipi: DummyIpi,
    nacl: DummyNacl,
    pmu: DummyPmu,
    srst: DummyReset,
    rfnc: DummyFence,
    sta: DummySta,
    susp: DummySusp,
    time: DummyTimer,
    info: DummyEnvInfo,
}

#[derive(RustSBI)]
struct TupleStruct(
    #[rustsbi(dbcn)] DummyConsole,
    #[rustsbi(cppc)] DummyCppc,
    #[rustsbi(hsm)] DummyHsm,
    #[rustsbi(ipi)] DummyIpi,
    #[rustsbi(nacl)] DummyNacl,
    #[rustsbi(pmu)] DummyPmu,
    #[rustsbi(srst)] DummyReset,
    #[rustsbi(rfnc)] DummyFence,
    #[rustsbi(sta)] DummySta,
    #[rustsbi(susp)] DummySusp,
    #[rustsbi(time)] DummyTimer,
    #[rustsbi(info)] DummyEnvInfo,
);

#[cfg(feature = "machine")]
#[derive(RustSBI)]
struct UnitStruct;

#[test]
fn rustsbi_impl_id() {
    let sbi = FullyImplemented {
        console: DummyConsole,
        cppc: DummyCppc,
        hsm: DummyHsm,
        ipi: DummyIpi,
        nacl: DummyNacl,
        pmu: DummyPmu,
        reset: DummyReset,
        fence: DummyFence,
        sta: DummySta,
        susp: DummySusp,
        timer: DummyTimer,
        info: DummyEnvInfo,
    };
    assert_eq!(sbi.handle_ecall(0x4E41434C, 0x0, [0; 6]), SbiRet::success(13));
    assert_eq!(sbi.handle_ecall(0x4E41434C, 0x1, [0; 6]), SbiRet::success(14));
    assert_eq!(sbi.handle_ecall(0x4E41434C, 0x2, [0; 6]), SbiRet::success(15));
    assert_eq!(sbi.handle_ecall(0x4E41434C, 0x3, [0; 6]), SbiRet::success(16));
    assert_eq!(sbi.handle_ecall(0x4E41434C, 0x4, [0; 6]), SbiRet::success(17));
    assert_eq!(sbi.handle_ecall(0x504D55, 0x0, [0; 6]), SbiRet::success(18));
    assert_eq!(sbi.handle_ecall(0x504D55, 0x1, [0; 6]), SbiRet::success(19));
    assert_eq!(sbi.handle_ecall(0x504D55, 0x2, [0; 6]), SbiRet::success(20));
    assert_eq!(sbi.handle_ecall(0x504D55, 0x3, [0; 6]), SbiRet::success(21));
    assert_eq!(sbi.handle_ecall(0x504D55, 0x4, [0; 6]), SbiRet::success(22));
    assert_eq!(sbi.handle_ecall(0x504D55, 0x5, [0; 6]), SbiRet::success(23));
    assert_eq!(sbi.handle_ecall(0x504D55, 0x6, [0; 6]), SbiRet::success(24));
    assert_eq!(sbi.handle_ecall(0x53525354, 0x0, [0; 6]), SbiRet::success(25));
    assert_eq!(sbi.handle_ecall(0x52464E43, 0x0, [0; 6]), SbiRet::success(26));
    assert_eq!(sbi.handle_ecall(0x52464E43, 0x1, [0; 6]), SbiRet::success(27));
    assert_eq!(sbi.handle_ecall(0x52464E43, 0x2, [0; 6]), SbiRet::success(28));
    assert_eq!(sbi.handle_ecall(0x52464E43, 0x3, [0; 6]), SbiRet::success(29));
    assert_eq!(sbi.handle_ecall(0x52464E43, 0x4, [0; 6]), SbiRet::success(30));
    assert_eq!(sbi.handle_ecall(0x52464E43, 0x5, [0; 6]), SbiRet::success(31));
    assert_eq!(sbi.handle_ecall(0x52464E43, 0x6, [0; 6]), SbiRet::success(32));
    let sbi = AlternateName {
        dbcn: DummyConsole,
        cppc: DummyCppc,
        hsm: DummyHsm,
        ipi: DummyIpi,
        nacl: DummyNacl,
        pmu: DummyPmu,
        srst: DummyReset,
        rfnc: DummyFence,
        sta: DummySta,
        susp: DummySusp,
        time: DummyTimer,
        info: DummyEnvInfo,
    };
    assert_eq!(sbi.handle_ecall(0x10, 0x1, [0; 6]).value, 4);
    let sbi = TupleStruct(
        DummyConsole,
        DummyCppc,
        DummyHsm,
        DummyIpi,
        DummyNacl,
        DummyPmu,
        DummyReset,
        DummyFence,
        DummySta,
        DummySusp,
        DummyTimer,
        DummyEnvInfo,
    );
    assert_eq!(sbi.handle_ecall(0x10, 0x1, [0; 6]).value, 4);
}

#[cfg(feature = "machine")]
#[test]
fn unit_struct() {
    let sbi = UnitStruct;
    assert_eq!(sbi.handle_ecall(0x10, 0x1, [0; 6]).value, 4);
}

#[test]
fn extension_impl() {
    let sbi = FullyImplemented {
        console: DummyConsole,
        cppc: DummyCppc,
        hsm: DummyHsm,
        ipi: DummyIpi,
        nacl: DummyNacl,
        pmu: DummyPmu,
        reset: DummyReset,
        fence: DummyFence,
        sta: DummySta,
        susp: DummySusp,
        timer: DummyTimer,
        info: DummyEnvInfo,
    };
    assert_eq!(
        sbi.handle_ecall(0x4442434E, 0x0, [0; 6]).error,
        -1isize as _
    );
}

struct DummyConsole;

impl rustsbi::Console for DummyConsole {
    fn write(&self, _: Physical<&[u8]>) -> SbiRet {
        // special return value for test cases
        SbiRet::failed()
    }

    fn read(&self, _: Physical<&mut [u8]>) -> SbiRet {
        unimplemented!()
    }

    fn write_byte(&self, _: u8) -> SbiRet {
        unimplemented!()
    }
}

struct DummyCppc;

impl rustsbi::Cppc for DummyCppc {
    fn probe(&self, _: u32) -> SbiRet {
        unimplemented!()
    }

    fn read(&self, _: u32) -> SbiRet {
        unimplemented!()
    }

    fn read_hi(&self, _: u32) -> SbiRet {
        unimplemented!()
    }

    fn write(&self, _: u32, _: u64) -> SbiRet {
        unimplemented!()
    }
}

struct DummyHsm;

impl rustsbi::Hsm for DummyHsm {
    fn hart_start(&self, _: usize, _: usize, _: usize) -> SbiRet {
        unimplemented!()
    }

    fn hart_stop(&self) -> SbiRet {
        unimplemented!()
    }

    fn hart_get_status(&self, _: usize) -> SbiRet {
        unimplemented!()
    }

    fn hart_suspend(&self, _: u32, _: usize, _: usize) -> SbiRet {
        unimplemented!()
    }
}

struct DummyIpi;

impl rustsbi::Ipi for DummyIpi {
    fn send_ipi(&self, _: HartMask) -> SbiRet {
        unimplemented!()
    }
}

struct DummyNacl;

impl rustsbi::Nacl for DummyNacl {
    fn probe_feature(&self, _: u32) -> SbiRet {
        SbiRet::success(13)
    }
    fn set_shmem(&self, _: SharedPtr<[u8; NATIVE]>, _: usize) -> SbiRet {
        SbiRet::success(14)
    }

    fn sync_csr(&self, _: usize) -> SbiRet {
        SbiRet::success(15)
    }

    fn sync_hfence(&self, _: usize) -> SbiRet {
        SbiRet::success(16)
    }

    fn sync_sret(&self) -> SbiRet {
        SbiRet::success(17)
    }
}

struct DummyPmu;

impl rustsbi::Pmu for DummyPmu {
    fn num_counters(&self) -> usize {
        18
    }

    fn counter_get_info(&self, _: usize) -> SbiRet {
        SbiRet::success(19)
    }

    fn counter_config_matching(&self, _: usize, _: usize, _: usize, _: usize, _: u64) -> SbiRet {
        SbiRet::success(20)
    }

    fn counter_start(&self, _: usize, _: usize, _: usize, _: u64) -> SbiRet {
        SbiRet::success(21)
    }

    fn counter_stop(&self, _: usize, _: usize, _: usize) -> SbiRet {
        SbiRet::success(22)
    }

    fn counter_fw_read(&self, _: usize) -> SbiRet {
        SbiRet::success(23)
    }

    fn counter_fw_read_hi(&self, _: usize) -> SbiRet {
        SbiRet::success(24)
    }
}

struct DummyReset;

impl rustsbi::Reset for DummyReset {
    fn system_reset(&self, _: u32, _: u32) -> SbiRet {
        SbiRet::success(25)
    }
}

struct DummyFence;

impl rustsbi::Fence for DummyFence {
    fn remote_fence_i(&self, _: HartMask) -> SbiRet {
        SbiRet::success(26)
    }

    fn remote_sfence_vma(&self, _: HartMask, _: usize, _: usize) -> SbiRet {
        SbiRet::success(27)
    }

    fn remote_sfence_vma_asid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet {
        SbiRet::success(28)
    }

    fn remote_hfence_gvma_vmid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet {
        SbiRet::success(29)
    }

    fn remote_hfence_gvma(&self, _: HartMask, _: usize, _: usize) -> SbiRet {
        SbiRet::success(30)
    }

    fn remote_hfence_vvma_asid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet {
        SbiRet::success(31)
    }

    fn remote_hfence_vvma(&self, _: HartMask, _: usize, _: usize) -> SbiRet {
        SbiRet::success(32)
    }
}

struct DummySta;

impl rustsbi::Sta for DummySta {
    fn set_shmem(&self, _: SharedPtr<[u8; 64]>, _: usize) -> SbiRet {
        unimplemented!()
    }
}

struct DummySusp;

impl rustsbi::Susp for DummySusp {
    fn system_suspend(&self, _: u32, _: usize, _: usize) -> SbiRet {
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
