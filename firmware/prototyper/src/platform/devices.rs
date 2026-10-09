//! Platform selection, binding, and ownership of firmware devices.

use alloc::boxed::Box;

use runtime::hart::HartWakeDevice;
use runtime::memory::MemoryRegistry;

use super::error::{self, ResultContext};
use super::info::BoardInfo;
use super::interrupts::InterruptController;
use crate::driver::{self, Console, ResetController};

/// Devices owned permanently by the platform after boot-hart initialization.
pub(super) struct Devices {
    pub(super) interrupts: InterruptController,
    pub(super) console: Option<Console>,
    pub(super) reset: Option<ResetController>,
    pub(super) hart_wake: Option<Box<dyn HartWakeDevice>>,
}

impl Devices {
    /// Selects and binds devices in their required initialization order.
    pub(super) fn bind(board: &BoardInfo, memory: &mut MemoryRegistry) -> error::Result<Self> {
        let interrupts = InterruptController::bind(board, memory)
            .during("binding interrupt devices and next-stage policy")?;
        if let Some(registers) = board.devices.interrupts.thead_plic {
            driver::thead::delegate_to_supervisor(registers, memory)
                .during("binding platform devices")?;
        }
        let console = board
            .devices
            .console
            .as_ref()
            .map(|console| Console::bind(console.registers, console.kind, console.clock_hz, memory))
            .transpose()
            .during("binding platform devices")?;
        let reset = board
            .devices
            .reset
            .bind(board.timebase_frequency_hz, memory)
            .during("binding platform devices")?;
        Ok(Self {
            interrupts,
            console,
            reset,
            hart_wake: None,
        })
    }
}
