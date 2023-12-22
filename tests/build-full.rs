use rustsbi::RustSBI;
use sbi_spec::{
    binary::{Physical, SbiRet, SharedPtr},
    nacl::shmem_size::NATIVE,
};

// This struct should pass Rust build.

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

struct DummyConsole;

impl rustsbi::Console for DummyConsole {
    fn write(&self, _: Physical<&[u8]>) -> SbiRet {
        unimplemented!()
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
}

struct DummyIpi;

impl rustsbi::Ipi for DummyIpi {
    fn send_ipi(&self, _: rustsbi::HartMask) -> SbiRet {
        unimplemented!()
    }
}

struct DummyNacl;

impl rustsbi::Nacl for DummyNacl {
    fn probe_feature(&self, _: u32) -> SbiRet {
        unimplemented!()
    }
    fn set_shmem(&self, _: SharedPtr<[u8; NATIVE]>, _: usize) -> SbiRet {
        unimplemented!()
    }

    fn sync_csr(&self, _: usize) -> SbiRet {
        unimplemented!()
    }

    fn sync_hfence(&self, _: usize) -> SbiRet {
        unimplemented!()
    }

    fn sync_sret(&self) -> SbiRet {
        unimplemented!()
    }
}

struct DummyPmu;

impl rustsbi::Pmu for DummyPmu {
    fn num_counters(&self) -> usize {
        unimplemented!()
    }

    fn counter_get_info(&self, _: usize) -> SbiRet {
        unimplemented!()
    }

    fn counter_config_matching(&self, _: usize, _: usize, _: usize, _: usize, _: u64) -> SbiRet {
        unimplemented!()
    }

    fn counter_start(&self, _: usize, _: usize, _: usize, _: u64) -> SbiRet {
        unimplemented!()
    }

    fn counter_stop(&self, _: usize, _: usize, _: usize) -> SbiRet {
        unimplemented!()
    }

    fn counter_fw_read(&self, _: usize) -> SbiRet {
        unimplemented!()
    }
}

struct DummyReset;

impl rustsbi::Reset for DummyReset {
    fn system_reset(&self, _: u32, _: u32) -> SbiRet {
        unimplemented!()
    }
}

struct DummyFence;

impl rustsbi::Fence for DummyFence {
    fn remote_fence_i(&self, _: rustsbi::HartMask) -> SbiRet {
        unimplemented!()
    }

    fn remote_sfence_vma(&self, _: rustsbi::HartMask, _: usize, _: usize) -> SbiRet {
        unimplemented!()
    }

    fn remote_sfence_vma_asid(&self, _: rustsbi::HartMask, _: usize, _: usize, _: usize) -> SbiRet {
        unimplemented!()
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
    assert_eq!(sbi.handle_ecall(0x10, 0x1, [0; 6]).value, 4);
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
}
