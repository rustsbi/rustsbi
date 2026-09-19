//! SiFive test finisher: board exit and reset signaling.
//!
//! # References
//!
//! - Platform source: [QEMU SiFive test-finisher implementation](https://github.com/qemu/qemu/blob/99e54ab5e7a6efc945af6d5661842155d1f3fc7a/hw/misc/sifive_test.c) —
//!   finisher register values and exit-code encoding.

use core::mem::{align_of, size_of};
use runtime::Result;
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};
use serde_device_tree::buildin::Node;

use crate::devicetree;

use super::registry::{self, BindResources, ResetDriver};
use super::{ResetBackend, ResetError, ResetReason, ResetRequest, ResetType};

#[repr(usize)]
#[derive(Clone, Copy)]
enum Register {
    Finish = 0x0000,
}

impl Register {
    const fn offset(self) -> usize {
        self as usize
    }
}

const SPAN: usize = Register::Finish.offset() + size_of::<u32>();
const COMPATIBLES: [&str; 1] = ["sifive,test0"];

/// Devicetree description for the SiFive test finisher.
#[derive(Default)]
pub(in crate::driver::reset) struct SifiveTestDriver {
    registers: Option<DeviceRegisterRange>,
}

impl ResetDriver for SifiveTestDriver {
    fn probe(
        &mut self,
        platform: &runtime::PlatformView<'_>,
        node: &Node<'_>,
        _parent: Option<&Node<'_>>,
    ) -> Result<()> {
        let Some(compatibles) = devicetree::compatible_strings(node) else {
            return Ok(());
        };
        if !compatibles
            .iter()
            .any(|compatible| COMPATIBLES.contains(&compatible))
        {
            return Ok(());
        }
        let registers = registry::primary_registers(platform, node)?;
        registry::set_once(&mut self.registers, registers)
    }

    fn has_device(&self) -> bool {
        self.registers.is_some()
    }

    fn bind(
        &self,
        resources: &mut BindResources<'_>,
    ) -> Result<alloc::boxed::Box<dyn ResetBackend>> {
        let registers = self.registers.ok_or(runtime::Error::InvalidArgs)?;
        Ok(alloc::boxed::Box::new(SifiveTestDevice::bind(
            registers,
            resources.memory(),
        )?))
    }

    fn log_summary(&self) {
        if let Some(registers) = self.registers {
            info!(
                "{:<30}: Available (Base Address: 0x{:x})",
                "Platform Reset Extension",
                registers.start().as_usize()
            );
        }
    }
}

#[repr(u16)]
enum FinishAction {
    Fail = 0x3333,
    Pass = 0x5555,
    Reset = 0x7777,
}

struct FinishCommand(u32);

impl FinishCommand {
    fn new(action: FinishAction, code: u16) -> Self {
        Self(u32::from(action as u16) | (u32::from(code) << u16::BITS))
    }

    fn for_request(req: ResetRequest) -> Option<Self> {
        match (req.reset_type(), req.reset_reason()) {
            (ResetType::Shutdown, ResetReason::NoReason) => Some(Self::new(FinishAction::Pass, 0)),
            (ResetType::Shutdown, ResetReason::SystemFailure) => {
                Some(Self::new(FinishAction::Fail, u16::MAX))
            }
            (
                ResetType::ColdReboot | ResetType::WarmReboot,
                ResetReason::NoReason | ResetReason::SystemFailure,
            ) => Some(Self::new(FinishAction::Reset, 0)),
            _ => None,
        }
    }
}

/// SiFive test device used by QEMU to exit or reset.
struct SifiveTestDevice {
    registers: MmioRegion,
}

impl SifiveTestDevice {
    fn bind(registers: DeviceRegisterRange, memory: &mut MemoryRegistry) -> runtime::Result<Self> {
        let registers = registers.subrange(0, SPAN)?;
        if !registers.start().is_aligned_to(align_of::<u32>()) {
            return Err(runtime::Error::InvalidArgs);
        }
        Ok(Self {
            registers: memory.acquire_mmio(registers)?,
        })
    }

    /// Writes the finish value and parks the hart until the board powers off.
    fn finish(&mut self, command: FinishCommand) -> ResetError {
        if self
            .registers
            .write(Register::Finish.offset(), command.0)
            .is_err()
        {
            return ResetError::Failed;
        }
        loop {
            riscv::asm::wfi();
        }
    }
}

impl ResetBackend for SifiveTestDevice {
    fn system_reset(&mut self, request: ResetRequest) -> ResetError {
        let Some(command) = FinishCommand::for_request(request) else {
            return ResetError::InvalidRequest;
        };
        self.finish(command)
    }
}
