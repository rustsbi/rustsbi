//! V861 C907 hart-release firmware driver.
//!
//! [`V861HartRelease`] implements the generic [`HartWake`] service with V861
//! reset-vector, clock, and power-control registers. It preserves the loader's
//! remaining C907 cache policy and restores that policy before a reset hart
//! enters the generic secondary-hart path.
//!
//! # References
//!
//! - Platform source: [V861 OpenSBI platform](https://github.com/YuzukiHD/opensbi/blob/c1ea219a901ff309e88e99c4b4e66ef9a55548de/platform/generic/allwinner/sun252i-v861.c)
//!   — hart-release register layout and power-on sequence.
//! - Reference implementation: [D1 cache save and restore](https://github.com/riscv-software-src/opensbi/blob/3593a5facc4c6938b90429a6973ba9ee21fc5899/platform/generic/allwinner/sun20i-d1.c)
//!   — C907 cache-policy preservation across reset.

use crate::driver::HartWake;
use runtime::hart::HartId;
use runtime::memory::{MemoryRegistry, MmioRegion};
use runtime::soc::allwinner::v861::{AllwinnerV861Soc, C907_HART_COUNT, C907CacheState};

const CLOCK_CONTROL_ENABLE: u32 = 0x0001_0001;
const RESET_VECTOR_MODE_MASK: u32 = 0x3 << 8;
const RESET_VECTOR_MODE_SHIFT: u32 = 8;
const RV32_MODE: u32 = 1;
const RV64_MODE: u32 = 2;
const POWER_CLAMP_RELEASE: u32 = 3 << 3;
const POWER_SWITCH_ON: u32 = 0x11;

/// V861's concrete implementation of the firmware hart-release service.
pub(crate) struct V861HartRelease {
    reset_vectors: [HartResetVector; C907_HART_COUNT],
    power_controls: [HartPowerControl; C907_HART_COUNT],
}

struct C907ClockControl(MmioRegion);

impl C907ClockControl {
    fn bind(soc: AllwinnerV861Soc, memory: &mut MemoryRegistry) -> runtime::Result<Self> {
        Ok(Self(memory.acquire_mmio(soc.c907_clock_control()?)?))
    }

    fn enable(&self) -> runtime::Result<()> {
        let value = u32::from_le(self.0.read::<u32>(0)?);
        self.0.write(0, (value | CLOCK_CONTROL_ENABLE).to_le())?;
        riscv::asm::fence();
        Ok(())
    }
}

struct HartResetVector(MmioRegion);

#[repr(usize)]
enum ResetVectorRegister {
    EntryLow = 0,
    EntryHigh = 4,
    Config = 8,
}

impl HartResetVector {
    fn bind(
        soc: AllwinnerV861Soc,
        hart: usize,
        memory: &mut MemoryRegistry,
    ) -> runtime::Result<Self> {
        Ok(Self(memory.acquire_mmio(soc.hart_reset_vector(hart)?)?))
    }

    fn program(&self, entry: u64) -> runtime::Result<()> {
        let mode = if cfg!(target_pointer_width = "64") {
            RV64_MODE
        } else {
            RV32_MODE
        };
        let config = u32::from_le(self.0.read::<u32>(ResetVectorRegister::Config as usize)?);
        let config = (config & !RESET_VECTOR_MODE_MASK) | (mode << RESET_VECTOR_MODE_SHIFT);
        self.0
            .write(ResetVectorRegister::Config as usize, config.to_le())?;
        self.0.write(
            ResetVectorRegister::EntryLow as usize,
            (entry as u32).to_le(),
        )?;
        self.0.write(
            ResetVectorRegister::EntryHigh as usize,
            ((entry >> u32::BITS) as u32).to_le(),
        )
    }
}

struct HartPowerControl(MmioRegion);

#[repr(usize)]
enum PowerRegister {
    Clamp = 0,
    Switch = 4,
}

impl HartPowerControl {
    fn bind(
        soc: AllwinnerV861Soc,
        hart: usize,
        memory: &mut MemoryRegistry,
    ) -> runtime::Result<Self> {
        Ok(Self(memory.acquire_mmio(soc.hart_power_control(hart)?)?))
    }

    fn power_on(&self) -> runtime::Result<()> {
        self.0
            .write(PowerRegister::Clamp as usize, POWER_CLAMP_RELEASE.to_le())?;
        self.0
            .write(PowerRegister::Switch as usize, POWER_SWITCH_ON.to_le())
    }
}

static CACHE_STATE: spin::Once<C907CacheState> = spin::Once::new();

impl V861HartRelease {
    pub(crate) fn bind(
        soc: AllwinnerV861Soc,
        memory: &mut MemoryRegistry,
    ) -> runtime::Result<Self> {
        C907ClockControl::bind(soc, memory)?.enable()?;
        let release = Self {
            reset_vectors: [
                HartResetVector::bind(soc, 0, memory)?,
                HartResetVector::bind(soc, 1, memory)?,
            ],
            power_controls: [
                HartPowerControl::bind(soc, 0, memory)?,
                HartPowerControl::bind(soc, 1, memory)?,
            ],
        };
        // boot0 leaves these C907 controls disabled. Runtime enables only the
        // firmware-owned bits and captures the loader policy for reset harts.
        CACHE_STATE.call_once(|| soc.initialize_c907_cache());
        Ok(release)
    }
}

impl HartWake for V861HartRelease {
    fn wake(&self, hart: HartId) -> runtime::Result<bool> {
        let hart = hart.as_usize();
        if hart >= C907_HART_COUNT {
            return Err(runtime::Error::InvalidArgs);
        }
        if crate::platform::hart_privilege_checked(hart) {
            return Ok(false);
        }
        let entry = crate::riscv::c907::reset_entry_address() as u64;
        self.reset_vectors[hart].program(entry)?;
        // Publish the reset vector and pending HSM request before power-on.
        riscv::asm::fence();
        self.power_controls[hart].power_on()?;
        riscv::asm::fence();
        Ok(true)
    }
}

pub(crate) extern "C" fn initialize_secondary() {
    CACHE_STATE
        .get()
        .expect("BUG: boot cache policy must be published before hart power-on")
        .restore();
    crate::secondary_hart(None);
}
