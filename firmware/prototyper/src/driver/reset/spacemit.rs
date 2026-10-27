//! SpacemiT P1 PMIC reset driver.
//!
//! # References
//!
//! - Reference implementation: [Linux P1 reboot driver](https://github.com/torvalds/linux/blob/2687c848e57820651b9f69d30c4710f4219f7dbf/drivers/power/reset/spacemit-p1-reboot.c)
//!   — power-control register and reset/shutdown bits used by K1 systems.

mod controller;

use alloc::boxed::Box;
use bitflags::bitflags;
use runtime::{
    Error, PlatformView, Result,
    memory::{DeviceRegisterRange, MemoryRegistry},
};

use crate::devicetree::EnabledNode;
use crate::driver::device::Driver;

use super::timebase_frequency_hz;
use super::{Description, Reset, ResetDevice, ResetError, ResetReason, ResetRequest, ResetType};

use controller::K1I2cController;

/// A validated 7-bit I2C device address.
#[derive(Clone, Copy)]
struct I2cAddress(u8);

impl I2cAddress {
    fn new(address: usize) -> Option<Self> {
        u8::try_from(address)
            .ok()
            .filter(|address| *address < 1 << 7)
            .map(Self)
    }

    const fn get(self) -> u8 {
        self.0
    }
}

#[repr(u8)]
enum Register {
    PowerControl2 = 0x7e,
}

bitflags! {
    struct PowerControl: u8 {
        const RESET = 1 << 1;
        const SHUTDOWN = 1 << 2;
    }
}

impl PowerControl {
    fn for_request(req: ResetRequest) -> Option<Self> {
        match (req.reset_type(), req.reset_reason()) {
            (ResetType::Shutdown, ResetReason::NoReason | ResetReason::SystemFailure) => {
                Some(Self::SHUTDOWN)
            }
            (
                ResetType::ColdReboot | ResetType::WarmReboot,
                ResetReason::NoReason | ResetReason::SystemFailure,
            ) => Some(Self::RESET),
            _ => None,
        }
    }
}

struct P1Pmic {
    i2c: K1I2cController,
    address: I2cAddress,
}

const PMIC_COMPATIBLES: [&str; 2] = ["spacemit,p1", "ky,spm8821"];
const I2C_COMPATIBLES: [&str; 2] = ["spacemit,k1-i2c", "ky,i2c"];

/// Devicetree description for the PMIC and its I2C controller.
#[derive(Clone, Copy)]
struct P1PmicDescription {
    controller: DeviceRegisterRange,
    address: I2cAddress,
}

fn description_from_node(
    platform: &PlatformView<'_>,
    discovered: EnabledNode<'_, '_>,
) -> Result<P1PmicDescription> {
    let node = discovered.node();

    // A PMIC child's `reg` value is an address on its parent I2C bus,
    // not a physical MMIO range, so it must not use `device_registers`.
    let mut address_entries = node.reg().ok_or(Error::InvalidArgs)?;
    let address_entry = address_entries.next().ok_or(Error::InvalidArgs)?;
    if address_entries.next().is_some() {
        return Err(Error::InvalidArgs);
    }
    let address =
        I2cAddress::new(address_entry.starting_address as usize).ok_or(Error::InvalidArgs)?;

    let parent = discovered.parent().ok_or(Error::InvalidArgs)?;
    let parent_compatibles = parent.compatible().ok_or(Error::InvalidArgs)?;
    if !parent_compatibles
        .all()
        .any(|compatible| I2C_COMPATIBLES.contains(&compatible))
    {
        return Err(Error::InvalidArgs);
    }
    let controller = platform
        .device_register(parent)?
        .ok_or(Error::InvalidArgs)?;
    Ok(P1PmicDescription {
        controller,
        address,
    })
}

/// Driver for the SpacemiT P1 PMIC reset path.
pub(in crate::driver::reset) struct P1PmicDriver;

pub(super) static P1_PMIC: P1PmicDriver = P1PmicDriver;

impl Driver<Reset> for P1PmicDriver {
    fn compatibles(&self) -> &'static [&'static str] {
        &PMIC_COMPATIBLES
    }

    fn parse<'a, 'tree>(
        &self,
        platform: &'a PlatformView<'tree>,
        device: EnabledNode<'a, 'tree>,
        _compatible: &str,
    ) -> Result<Option<Description>> {
        let description = description_from_node(platform, device)?;
        let registers = description.controller;
        let timebase = timebase_frequency_hz(platform);
        Ok(Some(Description::new(
            "spacemit_p1_pmic",
            registers,
            move |memory| {
                Ok(Box::new(P1Pmic::bind(
                    description.controller,
                    description.address,
                    timebase,
                    memory,
                )?) as Box<dyn ResetDevice>)
            },
        )))
    }
}

impl P1Pmic {
    fn bind(
        registers: DeviceRegisterRange,
        address: I2cAddress,
        timebase_frequency_hz: Option<u32>,
        memory: &mut MemoryRegistry,
    ) -> Result<Self> {
        Ok(Self {
            i2c: K1I2cController::bind(registers, timebase_frequency_hz, memory)?,
            address,
        })
    }

    fn set_power_control(&self, control: PowerControl) -> bool {
        let register = Register::PowerControl2 as u8;
        self.i2c
            .read_register(self.address, register)
            .is_some_and(|current| {
                self.i2c.write_register(
                    self.address,
                    register,
                    (PowerControl::from_bits_retain(current) | control).bits(),
                )
            })
    }

    fn park(&self) -> ! {
        loop {
            riscv::asm::wfi();
        }
    }
}

impl ResetDevice for P1Pmic {
    fn priority(&self) -> u32 {
        224
    }

    fn system_reset(&mut self, request: ResetRequest) -> ResetError {
        let Some(control) = PowerControl::for_request(request) else {
            return ResetError::InvalidRequest;
        };
        if !self.set_power_control(control) {
            error!("P1 PMIC: power-control transaction failed");
            return ResetError::Failed;
        }
        self.park()
    }
}
