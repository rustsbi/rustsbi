//! Device drivers.
//!
//! Platform devices are identified by Devicetree `compatible` strings
//! (e.g. `"sifive,clint0"`). Each driver declares the strings it supports
//! in a `*_COMPATIBLES` table. Platform discovery records the matching
//! register descriptions. [`bind_devices`] binds those descriptions to Runtime MMIO
//! windows and constructs the platform devices.

#![forbid(unsafe_code)]

mod aia;
mod cci;
mod clint;
mod console;
mod ipi;
mod reset;

use alloc::boxed::Box;

use runtime::memory::MemoryRegistry;

use crate::platform::BoardInfo;
use crate::riscv::csr::{mie, mip, stimecmp};
use crate::riscv::current_hartid;

pub(crate) use aia::{IMSIC_COMPATIBLES, IMSIC_FILE_SPAN, initialize_hart_imsic};
pub(crate) use cci::Cci550;
pub(crate) use clint::ClintKind;
pub(crate) use console::{ConsoleKind, DbcnBackend, DbcnError};
pub(crate) use ipi::{IpiBackend, IpiError, IpiRequest};

/// Hardware wakeup for DT-enabled harts that need not have entered firmware yet.
pub(crate) trait HartWake: Send {
    /// Returns true after requesting hardware wakeup, or false to use an IPI.
    fn wake(&mut self, hart_id: usize) -> runtime::Result<bool>;
}

pub(crate) use reset::{
    I2cAddress, P1_PMIC_COMPATIBLES, P1Pmic, PMIC_I2C_COMPATIBLES, ResetBackend, ResetError,
    ResetReason, ResetRequest, ResetType, SIFIVE_TEST_COMPATIBLES, SUNXI_WDT_V104_COMPATIBLE,
    SUNXI_WDT_V105_COMPATIBLE, SifiveTestDevice, SysconConfig, SysconPoweroff, SysconReboot,
};
pub(crate) use reset::{SunxiWdtV104, SunxiWdtV105};

pub(crate) const THEAD_PLIC_COMPATIBLES: [&str; 2] =
    ["thead,c900-plic", "allwinner,thead,c900-plic"];

/// Platform devices constructed from the discovered hardware description.
pub(crate) struct Devices {
    pub(crate) interrupts: Option<InterruptDevices>,
    pub(crate) console: Option<Box<dyn DbcnBackend + Send>>,
    pub(crate) sifive_test: Option<SifiveTestDevice>,
    pub(crate) spacemit_p1_pmic: Option<P1Pmic>,
    pub(crate) syscon_poweroff: Option<SysconPoweroff>,
    pub(crate) syscon_reboot: Option<SysconReboot>,
    pub(crate) sunxi_wdt_v104: Option<SunxiWdtV104>,
    pub(crate) sunxi_wdt_v105: Option<SunxiWdtV105>,
}

impl Devices {
    /// Returns whether firmware IPIs use IMSIC interrupt files.
    pub(crate) fn uses_imsic(&self) -> bool {
        self.interrupts
            .as_ref()
            .is_some_and(|devices| devices.ipi.is_imsic())
    }
}

/// Timer and IPI devices selected for the platform.
pub(crate) struct InterruptDevices {
    pub(crate) timer: Box<dyn TimerDevice>,
    pub(crate) ipi: Box<dyn IpiBackend + Send + Sync>,
}

/// Timer operations used by the SBI timer extension.
pub(crate) trait TimerDevice: Send {
    /// Reads the platform time counter.
    fn read_time(&self) -> u64;

    /// Programs the timer comparison value for `hart_id`.
    fn set_timer(&self, hart_id: usize, value: u64);
}

/// Timer implementation using the Sstc `stimecmp` CSR.
struct SstcTimer;

impl TimerDevice for SstcTimer {
    #[inline(always)]
    fn read_time(&self) -> u64 {
        riscv::register::time::read64()
    }

    #[inline(always)]
    fn set_timer(&self, hart_id: usize, value: u64) {
        if hart_id == current_hartid() {
            stimecmp::set(value);
            if value == u64::MAX {
                mip::clear_stimer();
                mie::clear_mtimer();
            }
        }
    }
}

fn bind_interrupts(
    board: &BoardInfo,
    memory: &mut MemoryRegistry,
) -> runtime::Result<Option<InterruptDevices>> {
    if let Some(imsic) = board.imsic.as_ref().filter(|_| aia::is_eligible(board)) {
        let aplic_config = if board.is_qemu_virt() {
            Some(crate::platform::qemu_aplic::QemuAplicConfig::new(
                board.machine_aplic.ok_or(runtime::Error::InvalidArgs)?,
                imsic.layout.machine_base,
                imsic.layout.hart_index_bits,
            )?)
        } else {
            warn!("AIA: skipping QEMU virt M-APLIC setup on '{}'", board.model);
            None
        };
        return aia::bind(imsic, aplic_config, memory).map(Some);
    }
    let Some(&(registers, kind)) = board.clint.as_ref() else {
        return Ok(None);
    };
    clint::bind(registers, kind, memory).map(Some)
}

/// Binds all devices selected during platform discovery.
pub(crate) fn bind_devices(
    board: &BoardInfo,
    memory: &mut MemoryRegistry,
) -> runtime::Result<Devices> {
    if let Some(registers) = board.thead_plic {
        // T-Head PLIC_CTRL bit 0 delegates access to S-mode,
        // only the boot hart binds this register.
        let control = memory.acquire_mmio(registers.subrange(0x1ffffc, size_of::<u32>())?)?;
        control.write(0, 1u32)?;
    }
    let interrupts = bind_interrupts(board, memory)?;
    let console = console::bind(board, memory)?;
    let sifive_test = board
        .reset
        .map(|registers| reset::sifive_test::bind(registers, memory))
        .transpose()?;
    let spacemit_p1_pmic = board
        .spacemit_p1_pmic_reset
        .map(|(registers, address)| {
            reset::pmic_spacemit_p1::bind(registers, address, board.timebase_frequency_hz, memory)
        })
        .transpose()?;
    let sunxi_wdt_v104 = board
        .sunxi_wdt_v104
        .map(|registers| {
            reset::sunxi_wdt_v104::bind(registers, board.timebase_frequency_hz, memory)
        })
        .transpose()?;
    let sunxi_wdt_v105 = board
        .sunxi_wdt_v105
        .map(|registers| reset::sunxi_wdt_v105::bind(registers, board.sunxi_rtc_v203_gprcm, memory))
        .transpose()?;
    // QEMU's SiFive finisher also exposes syscon aliases for the same word.
    let (syscon_poweroff, syscon_reboot) = reset::syscon::bind(
        board.syscon_poweroff.filter(|_| sifive_test.is_none()),
        board.syscon_reboot.filter(|_| sifive_test.is_none()),
        memory,
    )?;
    Ok(Devices {
        interrupts,
        console,
        sifive_test,
        spacemit_p1_pmic,
        syscon_poweroff,
        syscon_reboot,
        sunxi_wdt_v104,
        sunxi_wdt_v105,
    })
}
