//! SpacemiT P1 PMIC power-control device.

use bitflags::bitflags;
use core::convert::Infallible;
use runtime::{Error, Result, memory::MemoryRegistry};

use super::super::{ResetDevice, ResetReason, ResetRequest, ResetType};
use super::controller::K1I2cController;
use super::description::{Desc, I2cAddress};

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

/// A P1 PMIC bound to the K1 I2C controller that reaches it.
pub(super) struct P1Pmic {
    i2c: K1I2cController,
    address: I2cAddress,
}

impl P1Pmic {
    pub(super) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        let Desc {
            controller,
            address,
        } = description;
        Ok(Self {
            i2c: K1I2cController::init(controller, memory)?,
            address,
        })
    }

    fn set_power_control(&self, control: PowerControl) -> bool {
        // Preserve unrelated power-control bits instead of overwriting them.
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
            runtime::instruction::wait_for_interrupt();
        }
    }
}

impl ResetDevice for P1Pmic {
    fn system_reset(&mut self, request: ResetRequest) -> Result<Infallible> {
        let Some(control) = PowerControl::for_request(request) else {
            return Err(Error::InvalidArgs);
        };
        if !self.set_power_control(control) {
            error!("P1 PMIC: power-control transaction failed");
            return Err(Error::IoError);
        }
        self.park()
    }
}
