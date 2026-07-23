//! Validated SiFive test-device power control.

use alloc::boxed::Box;
use core::ops::Range;

use dtoolkit::fdt::Fdt;
use dtoolkit::{Node, Property};

use super::DriverError;
use crate::boot::BootInfo;
use crate::boot::device_tree::{BindingError, enabled, exact_node, model, reg_ranges};
use crate::config::TRUSTED_TARGET;
use crate::memory::{IoMem, IoMemError};
use crate::power::{Power, PowerDevice, PowerReason, RebootKind};

mod arch;

use arch::device_fence;

const COMPATIBLE: &str = "sifive,test0";
const QEMU_MODEL: &str = "riscv-virtio,qemu";
const QEMU_TEST_BASE: usize = 0x0010_0000;
const FAIL: u32 = 0x3333;
const PASS: u32 = 0x5555;
const RESET: u32 = 0x7777;

/// Validates and binds one whole-machine power-control register.
pub fn build(boot: &mut BootInfo, node_path: &str) -> Result<Power, DriverError> {
    let range = binding(boot, node_path)?;
    let io = IoMem::acquire(boot, range).map_err(|error| match error {
        IoMemError::InvalidRange | IoMemError::OutOfBounds | IoMemError::Misaligned => {
            DriverError::InvalidRange
        }
        IoMemError::AlreadyClaimed => DriverError::AlreadyOwned,
    })?;
    Power::new(Box::new(SifiveTest { io })).ok_or(DriverError::AlreadyOwned)
}

fn binding(boot: &BootInfo, path: &str) -> Result<Range<usize>, DriverError> {
    let fdt = Fdt::new(boot.dtb().as_bytes()).map_err(|_| DriverError::DeviceTree)?;
    let node = exact_node(&fdt, path).map_err(map_binding_error)?;
    if !enabled(&node)
        || !node
            .property("compatible")
            .is_some_and(|property| property.as_str_list().any(|value| value == COMPATIBLE))
    {
        return Err(DriverError::Unsupported);
    }
    let ranges = reg_ranges(node).map_err(map_binding_error)?;
    if ranges.len() != 1 {
        return Err(DriverError::InvalidRange);
    }
    let range = ranges.into_iter().next().unwrap();
    if !range.start.is_multiple_of(4) || range.end - range.start < 4 {
        return Err(DriverError::InvalidRange);
    }
    if !(model(&fdt) == QEMU_MODEL && range.start == QEMU_TEST_BASE) && !TRUSTED_TARGET {
        return Err(DriverError::Unauthorized);
    }
    Ok(range)
}

fn map_binding_error(error: BindingError) -> DriverError {
    match error {
        BindingError::DeviceTree => DriverError::DeviceTree,
        BindingError::Unsupported => DriverError::Unsupported,
        BindingError::InvalidRange => DriverError::InvalidRange,
    }
}

struct SifiveTest {
    io: IoMem,
}

impl SifiveTest {
    fn write(&self, value: u32) -> ! {
        device_fence();
        self.io
            .write_once(0, value)
            .unwrap_or_else(|_| crate::power::abort(|| {}));
        device_fence();
        // A platform that returns after accepting a power command has violated
        // the device contract. No firmware state is resumed after commit.
        loop {
            core::hint::spin_loop();
        }
    }
}

impl PowerDevice for SifiveTest {
    fn can_shutdown(&self, _reason: PowerReason) -> bool {
        true
    }

    fn can_reboot(&self, _kind: RebootKind, _reason: PowerReason) -> bool {
        true
    }

    fn shutdown(&self, reason: PowerReason) -> ! {
        match reason {
            PowerReason::Unspecified => self.write(PASS),
            PowerReason::SystemFailure => self.write(FAIL | (1 << 16)),
        }
    }

    fn reboot(&self, _kind: RebootKind, _reason: PowerReason) -> ! {
        self.write(RESET)
    }
}
