//! Specification: [RISC-V AIA 1.0], sections 4.5.2–4.5.4 and 4.5.11,
//! defines this register layout.
//!
//! QEMU 10.1 reads the hart-index width from the supervisor MSI address
//! register when routing supervisor interrupts, so that compatibility field is
//! repeated below. See QEMU's [APLIC implementation].
//!
//! [RISC-V AIA 1.0]: https://docs.riscv.org/reference/aia/_attachments/riscv-interrupts.pdf
//! [APLIC implementation]: https://github.com/qemu/qemu/blob/v10.1.0/hw/intc/riscv_aplic.c

use bitflags::bitflags;
use core::mem::size_of;

use runtime::{
    Error,
    memory::{MmioRegion, PhysAddr},
};

const PAGE_SHIFT: u32 = 12;
const PAGE_SIZE: usize = 1usize << PAGE_SHIFT;
const HIGH_PPN_WIDTH: u32 = 12;
const HART_INDEX_WIDTH_SHIFT: u32 = 12;
const HART_INDEX_WIDTH_FIELD_WIDTH: u32 = 4;
const MAX_HART_INDEX_BITS: u32 = (1 << HART_INDEX_WIDTH_FIELD_WIDTH) - 1;

#[derive(Clone, Copy)]
pub(super) struct EncodedMsiAddress {
    low: u32,
    high: u32,
}

impl EncodedMsiAddress {
    pub(super) fn machine(base: PhysAddr, hart_index_bits: u32) -> runtime::Result<Self> {
        Self::encode(base, hart_index_bits, true)
    }

    pub(super) fn supervisor(base: PhysAddr, hart_index_bits: u32) -> runtime::Result<Self> {
        // AIA defines LHXW in mmsiaddrcfgh. QEMU 10.1 instead takes it from
        // smsiaddrcfgh for an S-level domain, so repeat it for that emulator.
        Self::encode(base, hart_index_bits, true)
    }

    fn encode(
        base: PhysAddr,
        hart_index_bits: u32,
        encode_hart_index_width: bool,
    ) -> runtime::Result<Self> {
        if hart_index_bits > MAX_HART_INDEX_BITS || !base.is_aligned_to(PAGE_SIZE) {
            return Err(Error::InvalidArgs);
        }

        let base_ppn = base.as_usize() >> PAGE_SHIFT;
        if base_ppn & low_bits_mask(hart_index_bits) != 0
            || base_ppn >> u32::BITS > low_bits_mask(HIGH_PPN_WIDTH)
        {
            return Err(Error::InvalidArgs);
        }

        let mut high = (base_ppn >> u32::BITS) as u32;
        if encode_hart_index_width {
            high |= hart_index_bits << HART_INDEX_WIDTH_SHIFT;
        }
        Ok(Self {
            low: base_ppn as u32,
            high,
        })
    }
}

const fn low_bits_mask(width: u32) -> usize {
    (1usize << width) - 1
}

#[repr(usize)]
#[derive(Clone, Copy)]
enum Register {
    DomainConfig = 0x0000,
    MachineMsiAddressConfig = 0x1bc0,
    MachineMsiAddressConfigHigh = 0x1bc4,
    SupervisorMsiAddressConfig = 0x1bc8,
    SupervisorMsiAddressConfigHigh = 0x1bcc,
}

impl Register {
    const fn offset(self) -> usize {
        self as usize
    }
}

#[repr(usize)]
#[derive(Clone, Copy)]
enum RegisterArray {
    SourceConfig = 0x0004,
    ClearInterruptEnable = 0x1f00,
}

impl RegisterArray {
    fn element_offset(self, index: usize) -> usize {
        (self as usize)
            .checked_add(
                index
                    .checked_mul(size_of::<u32>())
                    .expect("BUG: APLIC register array index overflowed"),
            )
            .expect("BUG: APLIC register array offset overflowed")
    }
}

#[repr(u32)]
enum SourceConfig {
    DelegateToChildZero = 1 << 10,
}

bitflags! {
    struct MsiAddressConfigHigh: u32 {
        const LOCKED = 1 << 31;
    }
}

pub(super) struct AplicRegisters(MmioRegion);

impl AplicRegisters {
    pub(super) fn new(registers: MmioRegion) -> Self {
        Self(registers)
    }

    pub(super) fn configure_and_delegate_sources(
        &self,
        machine_msi: EncodedMsiAddress,
        supervisor_msi: EncodedMsiAddress,
        num_sources: usize,
    ) -> runtime::Result<bool> {
        let msi_configuration_locked = MsiAddressConfigHigh::from_bits_retain(
            self.read(Register::MachineMsiAddressConfigHigh)?,
        )
        .contains(MsiAddressConfigHigh::LOCKED);
        if !msi_configuration_locked {
            self.write_msi_address_config(
                Register::MachineMsiAddressConfig,
                Register::MachineMsiAddressConfigHigh,
                machine_msi,
            )?;
            self.write_msi_address_config(
                Register::SupervisorMsiAddressConfig,
                Register::SupervisorMsiAddressConfigHigh,
                supervisor_msi,
            )?;
        }

        // The M domain delivers nothing itself after all sources move to its
        // S-level child, so both domaincfg.IE and domaincfg.DM remain clear.
        self.write(Register::DomainConfig, 0)?;

        let enable_words = (num_sources + 1).div_ceil(u32::BITS as usize);
        for word in 0..enable_words {
            self.write_array(RegisterArray::ClearInterruptEnable, word, u32::MAX)?;
        }
        for index in 0..num_sources {
            self.write_array(
                RegisterArray::SourceConfig,
                index,
                SourceConfig::DelegateToChildZero as u32,
            )?;
        }
        Ok(msi_configuration_locked)
    }

    fn write_msi_address_config(
        &self,
        low_register: Register,
        high_register: Register,
        address: EncodedMsiAddress,
    ) -> runtime::Result<()> {
        self.write(low_register, address.low)?;
        self.write(high_register, address.high)
    }

    fn read(&self, register: Register) -> runtime::Result<u32> {
        self.0.read(register.offset())
    }

    fn write(&self, register: Register, value: u32) -> runtime::Result<()> {
        self.0.write(register.offset(), value)
    }

    fn write_array(
        &self,
        registers: RegisterArray,
        index: usize,
        value: u32,
    ) -> runtime::Result<()> {
        self.0.write(registers.element_offset(index), value)
    }
}
