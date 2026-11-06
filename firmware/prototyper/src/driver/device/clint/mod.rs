//! Machine timer and IPI drivers backed by a CLINT.
//!
//! Compatible strings follow the pinned [SiFive CLINT binding].
//!
//! [SiFive CLINT binding]: https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/Documentation/devicetree/bindings/timer/sifive%2Cclint.yaml

use core::mem::size_of;
use runtime::{Error, FdtNode, PlatformView, Result};

use super::{
    hart_slot::HartSlotMap,
    mswi,
    mtimer::{self, CompareAccess},
};

const SIFIVE_COMPATIBLES: [&str; 3] = ["riscv,clint0", "starfive,jh7110-clint", "sifive,clint0"];
const THEAD_COMPATIBLES: [&str; 1] = ["thead,c900-clint"];
const MTIMECMP_OFFSET: usize = 0x4000;
const MTIME_OFFSET: usize = 0xbff8;

/// Timer and software-IPI resources described by one CLINT node.
pub(crate) struct Desc {
    pub(crate) ipi: mswi::Desc,
    pub(crate) timer: mtimer::Desc,
}

/// Returns whether a compatible string names a supported CLINT.
pub(crate) fn supports(compatible: &str) -> bool {
    SIFIVE_COMPATIBLES.contains(&compatible) || THEAD_COMPATIBLES.contains(&compatible)
}

/// Parses a CLINT node into layouts consumed by shared IPI and timer drivers.
pub(crate) fn describe(platform: &PlatformView<'_>, node: FdtNode<'_, '_>) -> Result<Desc> {
    let registers = platform.device_register(node)?.ok_or(Error::InvalidArgs)?;
    let hart_slots = HartSlotMap::hart_id()?;
    let ipi_size = hart_slots
        .count()
        .checked_mul(size_of::<u32>())
        .ok_or(Error::Overflow)?;
    let compare_size = hart_slots
        .count()
        .checked_mul(size_of::<u64>())
        .ok_or(Error::Overflow)?;
    if ipi_size > MTIMECMP_OFFSET || compare_size > MTIME_OFFSET - MTIMECMP_OFFSET {
        return Err(Error::InvalidArgs);
    }
    let ipi = mswi::Desc {
        registers: registers.subrange(0, ipi_size)?,
        hart_slots: hart_slots.clone(),
        kind: "CLINT MSWI",
    };
    let compare = registers.subrange(MTIMECMP_OFFSET, compare_size)?;
    let sifive = node
        .compatible()
        .and_then(|compatibles| {
            compatibles.all().find_map(|compatible| match compatible {
                compatible if SIFIVE_COMPATIBLES.contains(&compatible) => Some(true),
                compatible if THEAD_COMPATIBLES.contains(&compatible) => Some(false),
                _ => None,
            })
        })
        .ok_or(Error::InvalidArgs)?;
    let timer = if sifive {
        mtimer::Desc {
            compare,
            time: Some(registers.subrange(MTIME_OFFSET, size_of::<u64>())?),
            hart_slots,
            access: CompareAccess::Native,
            kind: "SiFive CLINT",
        }
    } else {
        mtimer::Desc {
            compare,
            time: None,
            hart_slots,
            access: CompareAccess::TwoWords,
            kind: "T-Head CLINT",
        }
    };
    Ok(Desc { ipi, timer })
}
