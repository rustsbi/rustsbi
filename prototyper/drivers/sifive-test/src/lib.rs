//! Safe SiFive Test shutdown and reboot register protocol.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use core::ops::Range;

use machine::memory::{IoMem, IoMemError, io_fence};
use machine::power::{self, PowerControl, PowerInstallError, PowerReason, RebootKind};

/// Failure while binding the process-lifetime SiFive Test provider.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallError {
    /// The ordinary MMIO range is unavailable or invalid.
    Memory(IoMemError),
    /// Another power provider was already injected.
    Power(PowerInstallError),
}

/// Claims one SiFive Test register and injects it as the power provider.
pub fn install(range: Range<usize>) -> Result<(), InstallError> {
    let io = IoMem::acquire(range).map_err(InstallError::Memory)?;
    io.validate::<u32>(Register::CONTROL)
        .map_err(InstallError::Memory)?;
    power::inject(Box::new(SifiveTest { io })).map_err(InstallError::Power)
}

struct Register;

impl Register {
    /// QEMU/SiFive Test control register.
    const CONTROL: usize = 0;
    const FAIL: u32 = 0x3333;
    const PASS: u32 = 0x5555;
    const RESET: u32 = 0x7777;
}

struct SifiveTest {
    io: IoMem,
}

impl SifiveTest {
    fn write(&self, value: u32) {
        io_fence();
        let _ = self.io.write_once(Register::CONTROL, value);
        io_fence();
    }
}

impl PowerControl for SifiveTest {
    fn can_shutdown(&self, _reason: PowerReason) -> bool {
        true
    }

    fn can_reboot(&self, _kind: RebootKind, _reason: PowerReason) -> bool {
        true
    }

    fn shutdown(&self, reason: PowerReason) {
        match reason {
            PowerReason::Unspecified => self.write(Register::PASS),
            PowerReason::SystemFailure => self.write(Register::FAIL | (1 << 16)),
        }
    }

    fn reboot(&self, _kind: RebootKind, _reason: PowerReason) {
        self.write(Register::RESET)
    }
}
