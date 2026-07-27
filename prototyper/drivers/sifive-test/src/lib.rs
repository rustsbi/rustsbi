//! Safe SiFive Test shutdown and reboot register protocol.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use machine::power::{self, PowerControl, PowerReason, RebootKind};
use machine::{IoMem, io_fence};

/// Binds one already-owned SiFive Test register as the power provider.
pub fn bind(io: IoMem) -> bool {
    io.validate::<u32>(Register::CONTROL).is_ok()
        && power::inject(Box::new(SifiveTest { io })).is_some()
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
