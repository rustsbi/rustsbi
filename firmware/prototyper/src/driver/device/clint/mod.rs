//! CLINT timer and software-IPI discovery.
//!
//! The [CLINT binding] distinguishes the SiFive and T-Head layouts. The
//! [OpenSBI timer driver] documents the T-Head compare access width.
//!
//! [CLINT binding]: https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/Documentation/devicetree/bindings/timer/sifive%2Cclint.yaml
//! [OpenSBI timer driver]: https://github.com/riscv-software-src/opensbi/blob/v1.8/lib/utils/timer/fdt_timer_mtimer.c#L140-L154

use core::mem::size_of;
use runtime::{Error, FdtNode, PlatformView, Result};

use super::{
    hart_slot::HartSlotMap,
    mswi,
    mtimer::{self, CompareAccess},
};

pub(super) const COMPATIBLES: [&str; 4] = [
    "riscv,clint0",
    "starfive,jh7110-clint",
    "sifive,clint0",
    "thead,c900-clint",
];
const MTIMECMP_OFFSET: usize = 0x4000;
const MTIME_OFFSET: usize = 0xbff8;

/// Timer and software-IPI resources described by one CLINT node.
pub(crate) struct Desc {
    pub(crate) ipi: mswi::Desc,
    pub(crate) timer: mtimer::Desc,
}

/// Parses the timer and software-IPI layout for the matched `compatible`.
pub(super) fn describe(
    platform: &PlatformView<'_>,
    node: FdtNode<'_, '_>,
    compatible: &str,
) -> Result<Desc> {
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
    };
    let compare = registers.subrange(MTIMECMP_OFFSET, compare_size)?;
    let (time, access) = if compatible == "thead,c900-clint" {
        // T-Head CLINT lacks MMIO `mtime` and requires 32-bit compare accesses.
        (None, CompareAccess::TwoWords)
    } else {
        (
            Some(registers.subrange(MTIME_OFFSET, size_of::<u64>())?),
            CompareAccess::Native,
        )
    };
    let timer = mtimer::Desc {
        compare,
        time,
        hart_slots,
        access,
    };
    Ok(Desc { ipi, timer })
}
