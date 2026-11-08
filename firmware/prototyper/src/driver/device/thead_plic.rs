//! T-Head PLIC access delegation to the supervisor.

use runtime::{
    Result,
    memory::{MemoryRegistry, RegRange},
};

/// Compatible strings for the T-Head PLIC variants.
pub(crate) const COMPATIBLES: [&str; 2] = ["thead,c900-plic", "allwinner,thead,c900-plic"];

/// Delegates T-Head PLIC register access to the supervisor.
pub(crate) fn init(registers: RegRange, memory: &mut MemoryRegistry) -> Result<()> {
    let control = memory.acquire(registers.subrange(0x1ffffc, size_of::<u32>())?)?;
    control.write(0, 1u32)
}
