//! C907 startup and V861 power-on through the generic HSM wake backend.
//!
//! HSM serializes power requests. Once a hart has entered firmware, its
//! stop/start cycle uses the generic WFI/IPI path and preserves coherency.
//! Preserve the loader's cache policy and restore it on reset harts. Register
//! references: the [V861 platform] and [D1 cache save/restore].
//!
//! [V861 platform]: https://github.com/YuzukiHD/opensbi/blob/c1ea219a901ff309e88e99c4b4e66ef9a55548de/platform/generic/allwinner/sun252i-v861.c
//! [D1 cache save/restore]: https://github.com/riscv-software-src/opensbi/blob/3593a5facc4c6938b90429a6973ba9ee21fc5899/platform/generic/allwinner/sun20i-d1.c

use crate::{driver::HartWake, sbi::trap_stack};
use runtime::{
    AllwinnerV861Registers,
    memory::{MemoryRegistry, MmioRegion},
};

pub(crate) struct V861Wake {
    vectors: [MmioRegion; 2],
    power: [MmioRegion; 2],
}

pub(crate) fn initialize_boot_hart(
    description: AllwinnerV861Registers,
    memory: &mut MemoryRegistry,
) -> runtime::Result<V861Wake> {
    let clock = memory.acquire_mmio(description.clock_gate()?)?;
    let value = clock.read::<u32>(0)?;
    clock.write(0, value | 0x0001_0001u32)?;
    riscv::asm::fence();
    let wake = V861Wake {
        vectors: [
            memory.acquire_mmio(description.reset_vector(0)?)?,
            memory.acquire_mmio(description.reset_vector(1)?)?,
        ],
        power: [
            memory.acquire_mmio(description.power_control(0)?)?,
            memory.acquire_mmio(description.power_control(1)?)?,
        ],
    };
    CACHE_STATE.call_once(CacheState::read);
    Ok(wake)
}

static CACHE_STATE: spin::Once<CacheState> = spin::Once::new();

struct CacheState {
    l2: usize,
    status: usize,
    hint: usize,
    cache: usize,
}

impl CacheState {
    fn read() -> Self {
        let state;
        // SAFETY: selected only for a V861 C907 running in M-mode. Read
        // the policy established by boot0, without changing the boot hart.
        unsafe {
            let (l2, status, hint, cache);
            core::arch::asm!(
                "csrr {l2}, 0x7c3", "csrr {status}, 0x7c0",
                "csrr {hint}, 0x7c5", "csrr {cache}, 0x7c1",
                l2 = out(reg) l2, status = out(reg) status,
                hint = out(reg) hint, cache = out(reg) cache,
            );
            state = Self {
                l2,
                status,
                hint,
                cache,
            };
        }
        state
    }

    fn restore(&self) {
        // SAFETY: the hardware-reset C907 has invalidated its caches and
        // joined coherency before its first stack/shared-memory access.
        // Generic feature setup runs afterwards, including Sv32 MAEE setup.
        unsafe {
            core::arch::asm!(
                "csrw 0x7c3, {l2}", "csrw 0x7c0, {status}",
                "csrw 0x7c5, {hint}", "csrw 0x7c1, {cache}",
                l2 = in(reg) self.l2, status = in(reg) self.status,
                hint = in(reg) self.hint, cache = in(reg) self.cache,
            );
        }
    }
}

impl HartWake for V861Wake {
    fn wake(&mut self, hart: usize) -> runtime::Result<bool> {
        if hart >= 2 {
            return Err(runtime::Error::InvalidArgs);
        }
        if crate::platform::hart_privilege_checked(hart) {
            return Ok(false);
        }
        let entry = warm_entry as *const () as usize as u64;
        let mode = if cfg!(target_pointer_width = "64") {
            2u32
        } else {
            1
        };
        let config = self.vectors[hart].read::<u32>(8)?;
        self.vectors[hart].write(8, (config & !0x300) | (mode << 8))?;
        self.vectors[hart].write(0, entry as u32)?;
        self.vectors[hart].write(4, (entry >> 32) as u32)?;
        // Publish the reset vector and pending HSM request before power-on.
        riscv::asm::fence();
        self.power[hart].write(0, 3u32 << 3)?;
        self.power[hart].write(4, 0x11u32)?;
        riscv::asm::fence();
        Ok(true)
    }
}

/// Enters a powered-on C907 without a boot0 register envelope.
///
/// # Safety
/// Called only by V861 hardware reset after the boot hart publishes stacks,
/// platform state and HSM startup data. No stack is used before coherency.
#[unsafe(naked)]
unsafe extern "C" fn warm_entry() -> ! {
    core::arch::naked_asm!(
        ".balign 4",
        "csrw mie, zero",
        "li t0, 0x70013",
        "csrw 0x7c2, t0",
        "li t0, 1",
        "csrw 0x7f3, t0",
        "fence rw, rw",
        "call {locate}",
        "call {initialize}",
        "csrw mscratch, sp",
        "j {boot}",
        locate = sym trap_stack::locate,
        initialize = sym initialize_secondary,
        boot = sym crate::sbi::trap::boot::boot,
    )
}

extern "C" fn initialize_secondary() {
    CACHE_STATE
        .get()
        .expect("boot cache policy published before hart power-on")
        .restore();
    crate::secondary_hart(None);
}
