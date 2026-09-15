//! V821 A27L2 noncacheable physical memory attributes.

use core::arch::asm;
use runtime::AllwinnerV821Registers;

pub(crate) fn initialize(_soc: AllwinnerV821Registers, hart_count: usize) -> runtime::Result<()> {
    if runtime::hart::HartId::current()
        .expect("invalid current hart")
        .as_usize()
        != 0
        || hart_count != 1
        || riscv::register::mvendorid::read().bits() != 0x31e
    {
        return Err(runtime::Error::InvalidArgs);
    }
    // SAFETY: The V821 boot contract reserves PMA15 for the 4--8 GiB uncached alias;
    // preserve the other entries and configure it before supervisor entry.
    unsafe {
        let config: usize;
        asm!("csrr {value}, 0xbc3", value = out(reg) config, options(nomem, nostack));
        asm!("csrw 0xbc3, {config}", "csrw 0xbdf, {addr}", "fence",
            addr = in(reg) 0x5fff_ffffusize,
            config = in(reg) (config & !0xff00_0000) | 0x0f00_0000usize,
            options(nostack));
    }
    Ok(())
}
