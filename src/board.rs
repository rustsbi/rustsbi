//! Board support, including peripheral and core drivers.

use rustsbi::{Console, Physical, RustSBI, SbiRet};

#[derive(RustSBI, Default)]
#[rustsbi(dynamic)]
pub struct Board<'a> {
    #[rustsbi(console)]
    uart16550: Option<Uart16550Device<'a>>,
    #[rustsbi(ipi, timer)]
    clint: Option<ClintDevice<'a>>,
    #[rustsbi(reset)]
    sifive_test: Option<SifiveTestDevice<'a>>,
}

struct Uart16550Device<'a> {
    inner: &'a uart16550::Uart16550<u8>,
}

impl<'a> Console for Uart16550Device<'a> {
    #[inline]
    fn write(&self, bytes: Physical<&[u8]>) -> SbiRet {
        // TODO verify valid memory range for a `Physical` slice.
        let start = bytes.phys_addr_lo();
        let buf = unsafe { core::slice::from_raw_parts(start as *const u8, bytes.num_bytes()) };
        SbiRet::success(self.inner.write(buf))
    }
    #[inline]
    fn read(&self, _bytes: Physical<&mut [u8]>) -> SbiRet {
        todo!()
    }
    #[inline]
    fn write_byte(&self, byte: u8) -> SbiRet {
        self.inner.write(&[byte]);
        SbiRet::success(0)
    }
}

pub struct ClintDevice<'a> {
    pub clint: &'a aclint::SifiveClint,
    pub max_hart_id: usize,
}

impl<'a> rustsbi::Timer for ClintDevice<'a> {
    #[inline]
    fn set_timer(&self, stime_value: u64) {
        let current_hart_id = riscv::register::mhartid::read();
        self.clint.write_mtimecmp(current_hart_id, stime_value);
    }
}

impl<'a> rustsbi::Ipi for ClintDevice<'a> {
    #[inline]
    fn send_ipi(&self, hart_mask: rustsbi::HartMask) -> SbiRet {
        for hart_id in 0..=self.max_hart_id {
            if hart_mask.has_bit(hart_id) {
                self.clint.set_msip(hart_id);
            }
        }
        SbiRet::success(0)
    }
}

pub struct SifiveTestDevice<'a> {
    pub sifive_test: &'a sifive_test_device::SifiveTestDevice,
}

impl<'a> rustsbi::Reset for SifiveTestDevice<'a> {
    #[inline]
    fn system_reset(&self, reset_type: u32, reset_reason: u32) -> SbiRet {
        use rustsbi::spec::srst::{
            RESET_REASON_NO_REASON, RESET_REASON_SYSTEM_FAILURE, RESET_TYPE_COLD_REBOOT,
            RESET_TYPE_SHUTDOWN, RESET_TYPE_WARM_REBOOT,
        };
        match reset_type {
            RESET_TYPE_SHUTDOWN => match reset_reason {
                RESET_REASON_NO_REASON => self.sifive_test.pass(),
                RESET_REASON_SYSTEM_FAILURE => self.sifive_test.fail(-1 as _),
                value => self.sifive_test.fail(value as _),
            },
            RESET_TYPE_COLD_REBOOT | RESET_TYPE_WARM_REBOOT => {
                self.sifive_test.reset();
            }
            _ => SbiRet::invalid_param(),
        }
    }
}
