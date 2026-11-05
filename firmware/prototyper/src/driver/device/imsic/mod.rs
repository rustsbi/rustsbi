//! IMSIC interrupt files and the firmware's message sender.
//!
//! Runtime owns claiming and the CSR side of the IMSIC; this module only maps
//! the interrupt files described by the device tree and posts messages into
//! them.
//!
//! Specification: [RISC-V AIA 1.0](https://docs.riscv.org/reference/aia/v1.0/IMSIC.html),
//! sections 2.1.5 and 2.1.8 — IMSIC MMIO pages and interrupt-file setup.

mod description;
mod device;
mod parse;

use runtime::{Error, FdtNode, PlatformView, Result};

pub(crate) use description::{AddressLayout, Desc, IMSIC_COMPATIBLES, IMSIC_FILE_SPAN};

/// Parses one IMSIC node after resolving its register ranges.
pub(crate) fn describe(platform: &PlatformView<'_>, node: FdtNode<'_, '_>) -> Result<Option<Desc>> {
    let registers = platform.device_registers(node)?.ok_or(Error::InvalidArgs)?;
    parse::describe(platform, node, &registers)
}
