//! SpacemiT P1 PMIC reset driver.
//!
//! Compatibility reference: the pinned [Linux P1 reboot driver] defines the
//! power-control register and reset/shutdown bits used by existing K1 systems.
//!
//! [Linux P1 reboot driver]: https://github.com/torvalds/linux/blob/2687c848e57820651b9f69d30c4710f4219f7dbf/drivers/power/reset/spacemit-p1-reboot.c

mod controller;

use bitflags::bitflags;
use runtime::memory::{DeviceRegisterRange, MemoryRegistry};

use super::{ResetBackend, ResetError, ResetReason, ResetRequest, ResetType};

use controller::K1I2cController;

/// A validated 7-bit I2C device address.
#[derive(Clone, Copy)]
pub(crate) struct I2cAddress(u8);

impl I2cAddress {
    pub(crate) fn new(address: usize) -> Option<Self> {
        u8::try_from(address)
            .ok()
            .filter(|address| *address < 1 << 7)
            .map(Self)
    }

    pub(crate) const fn get(self) -> u8 {
        self.0
    }
}

#[repr(u8)]
enum Register {
    PowerControl2 = 0x7e,
}

bitflags! {
    pub(crate) struct PowerControl: u8 {
        const RESET = 1 << 1;
        const SHUTDOWN = 1 << 2;
    }
}

impl PowerControl {
    fn for_request(req: ResetRequest) -> Option<Self> {
        match (req.reset_type, req.reset_reason) {
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

pub(crate) struct P1Pmic {
    i2c: K1I2cController,
    address: I2cAddress,
}

pub(in crate::driver) fn bind(
    registers: DeviceRegisterRange,
    address: I2cAddress,
    timebase_frequency_hz: Option<u32>,
    memory: &mut MemoryRegistry,
) -> runtime::Result<P1Pmic> {
    Ok(P1Pmic {
        i2c: K1I2cController::bind(registers, timebase_frequency_hz, memory)?,
        address,
    })
}

impl P1Pmic {
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
    type Request = PowerControl;

    fn prepare_reset(&self, req: ResetRequest) -> Option<Self::Request> {
        PowerControl::for_request(req)
    }

    fn system_reset(&mut self, req: Self::Request) -> ResetError {
        if !self.set_power_control(req) {
            error!("P1 PMIC: power-control transaction failed");
            return ResetError::Failed;
        }
        self.park()
    }
}
