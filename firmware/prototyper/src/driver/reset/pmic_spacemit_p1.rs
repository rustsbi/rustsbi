//! SpacemiT P1 PMIC reset driver.
//!
//! # References
//!
//! - Reference implementation: [Linux P1 reboot driver](https://github.com/torvalds/linux/blob/2687c848e57820651b9f69d30c4710f4219f7dbf/drivers/power/reset/spacemit-p1-reboot.c)
//!   — power-control register and reset/shutdown bits used by K1 systems.

mod controller;

use bitflags::bitflags;
use runtime::memory::{DeviceRegisterRange, MemoryRegistry};
use runtime::{Error, Result};
use serde_device_tree::buildin::Node;

use crate::devicetree;

use super::registry::{self, BindResources, ResetDriver};
use super::{ResetBackend, ResetError, ResetReason, ResetRequest, ResetType};

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

#[derive(Default)]
pub(in crate::driver::reset) struct P1PmicDriver {
    description: Option<P1PmicDescription>,
}

impl ResetDriver for P1PmicDriver {
    fn probe(
        &mut self,
        platform: &runtime::PlatformView<'_>,
        node: &Node<'_>,
        parent: Option<&Node<'_>>,
    ) -> Result<()> {
        let Some(compatibles) = devicetree::compatible_strings(node) else {
            return Ok(());
        };
        if !compatibles
            .iter()
            .any(|compatible| PMIC_COMPATIBLES.contains(&compatible))
        {
            return Ok(());
        }

        // A PMIC child's `reg` value is an address on its parent I2C bus,
        // not a physical MMIO range, so it must not use `device_registers`.
        let addresses = node
            .get_prop("reg")
            .ok_or(Error::InvalidArgs)?
            .deserialize::<serde_device_tree::buildin::Reg>();
        let mut address_entries = addresses.iter();
        let address_entry = address_entries.next().ok_or(Error::InvalidArgs)?;
        if address_entries.next().is_some() {
            return Err(Error::InvalidArgs);
        }
        let address = I2cAddress::new(address_entry.0.start).ok_or(Error::InvalidArgs)?;

        let parent = parent.ok_or(Error::InvalidArgs)?;
        let parent_compatibles =
            devicetree::compatible_strings(parent).ok_or(Error::InvalidArgs)?;
        if !parent_compatibles
            .iter()
            .any(|compatible| I2C_COMPATIBLES.contains(&compatible))
        {
            return Err(Error::InvalidArgs);
        }
        let controller = registry::primary_registers(platform, parent)?;
        registry::set_once(
            &mut self.description,
            P1PmicDescription {
                controller,
                address,
            },
        )
    }

    fn has_device(&self) -> bool {
        self.description.is_some()
    }

    fn bind(
        &self,
        resources: &mut BindResources<'_>,
    ) -> Result<alloc::boxed::Box<dyn ResetBackend>> {
        let description = self.description.ok_or(Error::InvalidArgs)?;
        let timebase_frequency_hz = resources.timebase_frequency_hz();
        Ok(alloc::boxed::Box::new(P1Pmic::bind(
            description.controller,
            description.address,
            timebase_frequency_hz,
            resources.memory(),
        )?))
    }

    fn log_summary(&self) {
        if let Some(description) = self.description {
            info!(
                "{:<30}: Available (SpacemiT P1 PMIC @ 0x{:02x}, I2C Base: 0x{:x})",
                "Platform Reset Extension",
                description.address.get(),
                description.controller.start().as_usize()
            );
        }
    }
}

impl P1Pmic {
    fn bind(
        registers: DeviceRegisterRange,
        address: I2cAddress,
        timebase_frequency_hz: Option<u32>,
        memory: &mut MemoryRegistry,
    ) -> runtime::Result<Self> {
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

impl ResetBackend for P1Pmic {
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
