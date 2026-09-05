//! Validation and allocation of physical address ranges.

use alloc::vec::Vec;

use super::{
    DeviceRegisterRange, MmioRegion, PhysAddrRange, SupervisorMemory, image::locate_firmware_image,
};
use crate::{Error, Result};

/// Tracks the physical resources issued by Runtime.
///
/// [`crate::PlatformDescription::into_memory_resources`] supplies normalized RAM and reserved
/// ranges. The registry retains them, plus each issued MMIO window, so later
/// device bindings cannot acquire overlapping physical addresses.
pub struct MemoryRegistry {
    firmware_image_range: PhysAddrRange,
    ram_ranges: Vec<PhysAddrRange>,
    reserved_ranges: Vec<PhysAddrRange>,
    acquired_mmio: Vec<PhysAddrRange>,
}

impl MemoryRegistry {
    pub(crate) fn from_ranges(
        ram: impl IntoIterator<Item = PhysAddrRange>,
        reserved: impl IntoIterator<Item = PhysAddrRange>,
    ) -> Result<(SupervisorMemory, Self)> {
        Self::from_ranges_with_firmware(locate_firmware_image()?, ram, reserved)
    }

    fn from_ranges_with_firmware(
        firmware_image_range: PhysAddrRange,
        ram: impl IntoIterator<Item = PhysAddrRange>,
        reserved: impl IntoIterator<Item = PhysAddrRange>,
    ) -> Result<(SupervisorMemory, Self)> {
        let ram_ranges = normalize_ram_ranges(ram)?;
        let reserved_ranges = normalize_reserved_ranges(reserved);
        validate_memory_ranges(&ram_ranges, &reserved_ranges)?;

        let registry = Self {
            firmware_image_range,
            ram_ranges,
            reserved_ranges,
            acquired_mmio: Vec::new(),
        };
        let supervisor = registry.derive_supervisor_memory()?;
        Ok((supervisor, registry))
    }

    /// Returns the physical range occupied by this firmware image.
    pub fn firmware_image_range(&self) -> PhysAddrRange {
        self.firmware_image_range
    }

    /// Returns the RAM ranges described by the platform.
    pub fn ram_ranges(&self) -> impl Iterator<Item = PhysAddrRange> + '_ {
        self.ram_ranges.iter().copied()
    }

    /// Acquires a device-register window outside RAM and reserved memory.
    ///
    /// The range must not overlap a window acquired earlier from this registry.
    /// A successful acquisition is recorded for all later checks.
    pub fn acquire_mmio(&mut self, registers: DeviceRegisterRange) -> Result<MmioRegion> {
        let range = registers.physical_range();
        if self
            .ram_ranges
            .iter()
            .chain(&self.reserved_ranges)
            .copied()
            .any(|unavailable| unavailable.overlaps(range))
            || self
                .acquired_mmio
                .iter()
                .copied()
                .any(|acquired| acquired.overlaps(range))
        {
            return Err(Error::AccessDenied);
        }

        self.acquired_mmio.push(range);
        Ok(MmioRegion::new(range))
    }

    fn derive_supervisor_memory(&self) -> Result<SupervisorMemory> {
        if !self
            .ram_ranges
            .iter()
            .copied()
            .any(|ram| ram.contains(self.firmware_image_range))
        {
            return Err(Error::InvalidArgs);
        }

        let mut excluded = self.reserved_ranges.clone();
        excluded.push(self.firmware_image_range);

        let accessible: Vec<_> = self
            .ram_ranges
            .iter()
            .copied()
            .flat_map(|ram| ram.excluding(&excluded))
            .collect();
        if accessible.is_empty() {
            Err(Error::NotEnoughResources)
        } else {
            Ok(SupervisorMemory::new(accessible))
        }
    }
}

fn normalize_ram_ranges(
    ranges: impl IntoIterator<Item = PhysAddrRange>,
) -> Result<Vec<PhysAddrRange>> {
    let mut ranges: Vec<_> = ranges.into_iter().collect();
    ranges.sort_unstable_by_key(|range| range.start());
    let mut normalized: Vec<PhysAddrRange> = Vec::new();

    for range in ranges {
        let Some(previous) = normalized.last_mut() else {
            normalized.push(range);
            continue;
        };
        if previous.overlaps(range) {
            return Err(Error::InvalidArgs);
        }
        if previous.adjacent(range) {
            *previous = previous.join(range);
        } else {
            normalized.push(range);
        }
    }
    Ok(normalized)
}

fn normalize_reserved_ranges(
    ranges: impl IntoIterator<Item = PhysAddrRange>,
) -> Vec<PhysAddrRange> {
    let mut ranges: Vec<_> = ranges.into_iter().collect();
    ranges.sort_unstable_by_key(|range| range.start());
    let mut normalized: Vec<PhysAddrRange> = Vec::new();

    for range in ranges {
        match normalized.last_mut() {
            Some(previous) if previous.overlaps(range) || previous.adjacent(range) => {
                *previous = previous.join(range);
            }
            _ => normalized.push(range),
        }
    }
    normalized
}

fn validate_memory_ranges(
    ram_ranges: &[PhysAddrRange],
    reserved_ranges: &[PhysAddrRange],
) -> Result<()> {
    for reserved in reserved_ranges.iter().copied() {
        for ram in ram_ranges.iter().copied() {
            if ram.overlaps(reserved) && !ram.contains(reserved) {
                return Err(Error::InvalidArgs);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::PhysAddr;

    fn range(start: usize, len: usize) -> PhysAddrRange {
        PhysAddrRange::from_start_len(PhysAddr::new(start), len).unwrap()
    }

    fn registers(start: usize, len: usize) -> DeviceRegisterRange {
        DeviceRegisterRange::from_description(range(start, len))
    }

    #[test]
    fn supervisor_memory_excludes_firmware_and_reserved_ranges() {
        let firmware = range(0x1200, 0x200);
        let reserved = range(0x1780, 0x100);
        let (memory, registry) = MemoryRegistry::from_ranges_with_firmware(
            firmware,
            [range(0x1800, 0x800), range(0x1000, 0x800)],
            [reserved],
        )
        .unwrap();

        assert_eq!(registry.firmware_image_range(), firmware);
        assert_eq!(memory.check_range(range(0x1000, 0x200)), Ok(()));
        assert_eq!(memory.check_range(firmware), Err(Error::InvalidArgs));
        assert_eq!(memory.check_range(reserved), Err(Error::InvalidArgs));
        assert_eq!(memory.check_range(range(0x1880, 0x780)), Ok(()));
        assert_eq!(
            memory.check_range(range(0x1100, 0x200)),
            Err(Error::InvalidArgs)
        );
    }

    #[test]
    fn registration_rejects_conflicting_or_uncontained_ranges() {
        assert!(matches!(
            MemoryRegistry::from_ranges_with_firmware(
                range(0x1000, 0x100),
                [range(0x1000, 0x800), range(0x1400, 0x800)],
                core::iter::empty(),
            ),
            Err(Error::InvalidArgs)
        ));
        assert!(matches!(
            MemoryRegistry::from_ranges_with_firmware(
                range(0x3000, 0x100),
                [range(0x1000, 0x1000)],
                core::iter::empty(),
            ),
            Err(Error::InvalidArgs)
        ));
        assert!(matches!(
            MemoryRegistry::from_ranges_with_firmware(
                range(0x1000, 0x100),
                [range(0x1000, 0x1000)],
                [range(0x1f00, 0x200)],
            ),
            Err(Error::InvalidArgs)
        ));
    }

    #[test]
    fn mmio_windows_are_disjoint_from_memory_and_each_other() {
        let (_, mut registry) = MemoryRegistry::from_ranges_with_firmware(
            range(0x1200, 0x100),
            [range(0x1000, 0x1000)],
            [range(0x3080, 0x10)],
        )
        .unwrap();

        registry.acquire_mmio(registers(0x3000, 0x40)).unwrap();
        assert!(matches!(
            registry.acquire_mmio(registers(0x3030, 0x20)),
            Err(Error::AccessDenied)
        ));
        assert!(matches!(
            registry.acquire_mmio(registers(0x3080, 0x10)),
            Err(Error::AccessDenied)
        ));
        assert!(registry.acquire_mmio(registers(0x3090, 0x70)).is_ok());
        assert!(matches!(
            registry.acquire_mmio(registers(0x1f00, 0x200)),
            Err(Error::AccessDenied)
        ));
    }
}
