//! V861 C907 secondary-hart wakeup.
//!
//! Reset-hart entry restores the loader's C907 cache policy before joining
//! the generic secondary-hart path.
//!
//! # References
//!
//! - Platform source: [V861 OpenSBI platform](https://github.com/YuzukiHD/opensbi/blob/c1ea219a901ff309e88e99c4b4e66ef9a55548de/platform/generic/allwinner/sun252i-v861.c)
//!   — hart-release register layout and power-on sequence.
//! - Reference implementation: [D1 cache save and restore](https://github.com/riscv-software-src/opensbi/blob/3593a5facc4c6938b90429a6973ba9ee21fc5899/platform/generic/allwinner/sun20i-d1.c)
//!   — cache-policy preservation across reset.

use runtime::hart::{HartId, HartWakeDevice};
use runtime::memory::{MemoryRegistry, MmioRegion, PhysAddr};
use runtime::soc::allwinner::v861::{AllwinnerV861Soc, C907_HART_COUNT};

const CLOCK_CONTROL_ENABLE: u32 = 0x0001_0001;
const RESET_VECTOR_MODE_MASK: u32 = 0x3 << 8;
const RESET_VECTOR_MODE_SHIFT: u32 = 8;
const RV32_MODE: u32 = 1;
const RV64_MODE: u32 = 2;
const POWER_CLAMP_RELEASE: u32 = 3 << 3;
const POWER_SWITCH_ON: u32 = 0x11;

/// V861 C907 hart-wakeup device using reset-vector and power-control registers.
pub(crate) struct V861HartWake {
    reset_vectors: [HartResetVector; C907_HART_COUNT],
    power_controls: [HartPowerControl; C907_HART_COUNT],
    reset_entry: runtime::boot::ResetEntry,
}

impl V861HartWake {
    pub(crate) fn bind(
        soc: AllwinnerV861Soc,
        memory: &mut MemoryRegistry,
    ) -> runtime::Result<Self> {
        enable_c907_clock(soc, memory)?;
        let reset_vectors = [
            HartResetVector::bind(soc, 0, memory)?,
            HartResetVector::bind(soc, 1, memory)?,
        ];
        let power_controls = [
            HartPowerControl::bind(soc, 0, memory)?,
            HartPowerControl::bind(soc, 1, memory)?,
        ];
        // Runtime retains the boot cache policy and restores it on reset harts.
        let reset_entry = soc
            .register_reset_entry(crate::boot::initialize_reset_hart)
            .expect("BUG: V861 C907 reset entry registered more than once");
        Ok(Self {
            reset_vectors,
            power_controls,
            reset_entry,
        })
    }
}

impl HartWakeDevice for V861HartWake {
    fn wake(&self, hart: HartId) -> runtime::Result<bool> {
        let hart = hart.as_usize();
        if hart >= C907_HART_COUNT {
            return Err(runtime::Error::InvalidArgs);
        }
        if crate::platform::hart_privilege_checked(hart) {
            return Ok(false);
        }
        self.reset_vectors[hart].program(self.reset_entry.address())?;
        self.power_controls[hart].power_on()?;
        Ok(true)
    }
}

fn enable_c907_clock(soc: AllwinnerV861Soc, memory: &mut MemoryRegistry) -> runtime::Result<()> {
    let registers = memory.acquire_mmio(soc.c907_clock_control()?)?;
    let value = u32::from_le(registers.read::<u32>(0)?);
    registers.write(0, (value | CLOCK_CONTROL_ENABLE).to_le())?;
    registers.synchronize();
    Ok(())
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

    fn program(&self, entry: PhysAddr) -> runtime::Result<()> {
        let entry = entry.as_usize() as u64;
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
        )?;
        // Publish the reset vector and pending HSM request before power-on.
        self.0.synchronize();
        Ok(())
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
            .write(PowerRegister::Switch as usize, POWER_SWITCH_ON.to_le())?;
        self.0.synchronize();
        Ok(())
    }
}
