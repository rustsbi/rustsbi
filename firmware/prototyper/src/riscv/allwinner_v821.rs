//! A27L2 cache maintenance for the V821 BSP's Andes SBI ABI.
//!
//! Command encodings and ordering follow the V821 SPL ads_rv32/mmu.c;
//! the range ABI is physical start/length as used by its A27L2 U-Boot.

use core::arch::asm;
use runtime::rustsbi::SbiRet;
use runtime::{
    AllwinnerV821Registers,
    memory::{
        DeviceRegisterRange, MemoryRegistry, MmioRegion, PhysAddr, PhysAddrRange, SupervisorMemory,
    },
};
use spin::{Mutex, Once};

pub(crate) const EXTENSION: usize = 0x0900_031e;
static CACHE: Once<Mutex<V821Cache>> = Once::new();
static USB_DMA_BYPASS: Once<Mutex<MmioRegion>> = Once::new();
pub(crate) const AWBASE_EXTENSION: usize = 0x5445_5335;

struct V821Cache {
    l2: MmioRegion,
}

pub(crate) fn initialize(
    _soc: AllwinnerV821Registers,
    registers: DeviceRegisterRange,
    usb_dma_bypass: Option<DeviceRegisterRange>,
    memory: &mut MemoryRegistry,
    hart_count: usize,
) -> runtime::Result<()> {
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
    let l2 = memory.acquire_mmio(registers)?;
    let control = l2.read::<u32>(8)?;
    l2.write(8, control | (1 << 13) | (1 << 10) | 1)?;
    riscv::asm::fence();
    CACHE.call_once(|| Mutex::new(V821Cache { l2 }));
    if let Some(registers) = usb_dma_bypass {
        let region = memory.acquire_mmio(registers)?;
        USB_DMA_BYPASS.call_once(|| Mutex::new(region));
    }
    Ok(())
}

/// V821 AWBASE USB enable sets DMA_WORDADD_BYPASS after Linux enables its clocks.
pub(crate) fn handle_awbase(function: usize) -> SbiRet {
    let Some(register) = USB_DMA_BYPASS.get() else {
        return SbiRet::not_supported();
    };
    if function != 11 {
        return SbiRet::not_supported();
    }
    riscv::asm::fence();
    match register.lock().write(0, 1u32) {
        Ok(()) => {
            riscv::asm::fence();
            SbiRet::success(0)
        }
        Err(_) => SbiRet::failed(),
    }
}

pub(crate) fn awbase_available() -> bool {
    USB_DMA_BYPASS.get().is_some()
}

pub(crate) fn available() -> bool {
    CACHE.get().is_some()
}

pub(crate) fn handle(function: usize, args: [usize; 6], memory: &SupervisorMemory) -> SbiRet {
    let Some(cache) = CACHE.get() else {
        return SbiRet::not_supported();
    };
    let mut cache = cache.lock();
    match function {
        0 | 1 => {
            let value;
            // SAFETY: the DTB-selected A27L2 was verified at initialization.
            unsafe {
                if function == 0 {
                    asm!("csrr {v}, 0x7ca", v = out(reg) value, options(nomem, nostack));
                } else {
                    asm!("csrr {v}, 0x7d0", v = out(reg) value, options(nomem, nostack));
                }
            }
            return SbiRet::success(value);
        }
        23 | 27 | 28 => {}
        _ => return SbiRet::not_supported(),
    }
    let result = if function == 23 {
        cache.flush_all()
    } else {
        cache.range(args[0], args[1], function == 28, memory)
    };
    match result {
        Ok(()) => SbiRet::success(0),
        Err(runtime::Error::InvalidArgs | runtime::Error::Overflow) => SbiRet::invalid_param(),
        Err(_) => SbiRet::failed(),
    }
}

impl V821Cache {
    fn l2_command(&mut self, command: u32) -> runtime::Result<()> {
        riscv::asm::fence();
        self.l2.write(0x40, command)?;
        let timer = crate::driver::timer::get().ok_or(runtime::Error::NotEnoughResources)?;
        let time = || {
            timer
                .read_time_low()
                .map(|value| value as u32)
                .ok_or(runtime::Error::NotEnoughResources)
        };
        let start = time()?;
        loop {
            match self.l2.read::<u32>(0x80)? & 0xf {
                0 => {
                    riscv::asm::fence();
                    return Ok(());
                }
                1 if time()?.wrapping_sub(start) < 4_000_000 => {}
                _ => return Err(runtime::Error::NotEnoughResources),
            }
            core::hint::spin_loop();
        }
    }

    fn flush_all(&mut self) -> runtime::Result<()> {
        // SAFETY: A27L2 command 6 writes back dirty L1 lines before invalidating.
        unsafe {
            asm!("fence", "csrw 0x80c, {cmd}", cmd = in(reg) 6usize, options(nostack));
        }
        self.l2_command(0x12)
    }

    fn range(
        &mut self,
        start: usize,
        length: usize,
        invalidate: bool,
        memory: &SupervisorMemory,
    ) -> runtime::Result<()> {
        if length == 0 {
            return Ok(());
        }
        let end = start.checked_add(length).ok_or(runtime::Error::Overflow)?;
        let first = start & !63;
        let last = end.checked_add(63).ok_or(runtime::Error::Overflow)? & !63;
        memory.check_range(PhysAddrRange::new(
            PhysAddr::new(first),
            PhysAddr::new(last),
        )?)?;
        // M-mode cache addresses are physical even when the supervisor enables satp.
        if !invalidate && length >= 128 * 1024 {
            return self.flush_all();
        }
        for address in (first..last).step_by(64) {
            let partial = address < start || end - address < 64;
            let (l1, l2) = if !invalidate {
                (1usize, 9u32)
            } else if partial {
                (2, 10)
            } else {
                (0, 8)
            };
            // SAFETY: the full cache-line span was checked against supervisor RAM;
            // partial lines retain surrounding dirty bytes by writeback/invalidate.
            unsafe {
                asm!("fence", "csrw 0x80b, {addr}", "csrw 0x80c, {cmd}",
                addr = in(reg) address, cmd = in(reg) l1, options(nostack));
            }
            self.l2.write(0x48, address as u32)?;
            self.l2_command(l2)?;
        }
        Ok(())
    }
}
