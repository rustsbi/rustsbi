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
pub(crate) mod ipi;
mod plmt;
mod reset;
pub(crate) mod timer;

use alloc::boxed::Box;

use runtime::memory::MemoryRegistry;

use crate::platform::{BoardInfo, ImsicInfo};

pub(crate) use aia::ImsicInterrupt;
pub(crate) use aia::{IMSIC_COMPATIBLES, IMSIC_FILE_SPAN, initialize_hart_imsic};
pub(crate) use cci::Cci550;
pub(crate) use clint::ClintKind;
pub(crate) use console::{ConsoleKind, DbcnBackend, DbcnError};
pub(crate) use ipi::{IpiBackend, IpiError, IpiRequest};
use timer::SstcTimer;
pub(crate) use timer::TimerBackend;

pub(crate) use runtime::hart::HartWake;

pub(crate) use reset::{
    I2cAddress, P1_PMIC_COMPATIBLES, P1Pmic, PMIC_I2C_COMPATIBLES, ResetBackend, ResetError,
    ResetReason, ResetRequest, ResetType, SIFIVE_TEST_COMPATIBLES, SUNXI_WDT_V104_COMPATIBLES,
    SUNXI_WDT_V105_COMPATIBLE, SifiveTestDevice, SysconConfig, SysconPoweroff, SysconReboot,
};
pub(crate) use reset::{SunxiWdtV104, SunxiWdtV105};

pub(crate) const PLMT_COMPATIBLE: &str = "andestech,plmt0";
pub(crate) const SUNXI_PLICSW_COMPATIBLE: &str = "allwinner,sun300i-plicsw";

pub(crate) const THEAD_PLIC_COMPATIBLES: [&str; 2] =
    ["thead,c900-plic", "allwinner,thead,c900-plic"];

/// Platform devices constructed from the discovered hardware description.
pub(crate) struct Devices {
    pub(crate) timer: Option<Box<dyn TimerBackend>>,
    pub(crate) ipi: Option<Box<dyn IpiBackend + Send + Sync>>,
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
        self.ipi.as_ref().is_some_and(|ipi| ipi.is_imsic())
    }
}

type InterruptDevices = (
    Option<Box<dyn TimerBackend>>,
    Option<Box<dyn IpiBackend + Send + Sync>>,
);

fn bind_interrupts(
    board: &BoardInfo,
    selected_imsic: Option<&ImsicInfo>,
    memory: &mut MemoryRegistry,
) -> runtime::Result<InterruptDevices> {
    if let Some(imsic) = selected_imsic {
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
        let (timer, ipi) = aia::bind(imsic, aplic_config, memory)?;
        return Ok((Some(timer), Some(ipi)));
    }
    if let (Some(plmt), Some(plicsw)) = (board.plmt, board.plicsw) {
        let hart_count = board
            .enabled_harts
            .iter()
            .rposition(|enabled| *enabled)
            .map(|last| last + 1)
            .ok_or(runtime::Error::InvalidArgs)?;
        if let Some(soc) = board.allwinner_v821 {
            let clock = memory.acquire_mmio(soc.plmt_clock()?)?;
            let value = u32::from_le(clock.read::<u32>(0)?);
            clock.write(0, (value | 0x80000000).to_le())?;
            riscv::asm::fence();
        }
        return Ok((
            Some(Box::new(plmt::bind(plmt, memory, hart_count)?)),
            Some(Box::new(ipi::plicsw::bind(plicsw, memory, hart_count)?)),
        ));
    }
    let Some(&(registers, kind)) = board.clint.as_ref() else {
        return Ok((None, None));
    };
    let (timer, ipi) = clint::bind(registers, kind, memory)?;
    Ok((Some(timer), Some(ipi)))
}

/// Binds all devices selected during platform discovery.
pub(crate) fn bind_devices(
    board: &BoardInfo,
    selected_imsic: Option<&ImsicInfo>,
    memory: &mut MemoryRegistry,
) -> runtime::Result<Devices> {
    if let Some(registers) = board.thead_plic {
        // T-Head PLIC_CTRL bit 0 delegates access to S-mode.
        let control = memory.acquire_mmio(registers.subrange(0x1ffffc, size_of::<u32>())?)?;
        control.write(0, 1u32)?;
    }
    let (timer, ipi) = bind_interrupts(board, selected_imsic, memory)?;
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
        timer,
        ipi,
        console,
        sifive_test,
        spacemit_p1_pmic,
        syscon_poweroff,
        syscon_reboot,
        sunxi_wdt_v104,
        sunxi_wdt_v105,
    })
}
