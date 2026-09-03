//! Registration of RAM and MMIO ranges.

use alloc::vec::Vec;

use super::{MmioRegion, MmioValue, PhysAddrRange, SupervisorMemory};
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RangeKind {
    Ram,
    Reserved,
    Mmio,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RegisteredRange {
    range: PhysAddrRange,
    kind: RangeKind,
}

impl RegisteredRange {
    const fn new(range: PhysAddrRange, kind: RangeKind) -> Self {
        Self { range, kind }
    }
}

/// Physical ranges registered for one firmware instance.
///
/// The registry is local state; Runtime does not install a global instance.
/// Supervisor RAM is handed out once, and overlapping MMIO windows are never
/// handed out independently.
pub struct MemoryRegistry {
    ranges: Vec<RegisteredRange>,
    acquired_mmio: Vec<PhysAddrRange>,
    supervisor_memory_acquired: bool,
}

impl MemoryRegistry {
    /// Registers the platform's RAM, reserved, and MMIO ranges.
    ///
    /// Adjacent ranges in the same input group are combined. A reserved range
    /// may be contained in RAM or MMIO; every other overlap is rejected.
    /// Returns [`Error::InvalidArgs`] if the ranges violate these rules.
    ///
    /// # Safety
    ///
    /// 1. Every non-reserved RAM or MMIO address must identify the stated
    ///    resource and remain accessible for the firmware's lifetime.
    /// 2. Access to returned supervisor RAM must not conflict with live Rust
    ///    references. The reserved input must exclude firmware allocations and
    ///    statics.
    /// 3. Side effects from any MMIO access made through a returned handle must
    ///    not violate Rust memory safety.
    /// 4. No other registry may independently hand out overlapping RAM or MMIO
    ///    access.
    pub unsafe fn from_ranges(
        ram: impl IntoIterator<Item = PhysAddrRange>,
        reserved: impl IntoIterator<Item = PhysAddrRange>,
        mmio: impl IntoIterator<Item = PhysAddrRange>,
    ) -> Result<Self> {
        Ok(Self {
            ranges: normalize(ram, reserved, mmio)?,
            acquired_mmio: Vec::new(),
            supervisor_memory_acquired: false,
        })
    }

    /// Returns the registered supervisor RAM, excluding reserved ranges.
    ///
    /// This succeeds once. It returns [`Error::AccessDenied`] after the handle
    /// has been issued and [`Error::NotEnoughResources`] if no RAM remains.
    pub fn acquire_supervisor_memory(&mut self) -> Result<SupervisorMemory> {
        if self.supervisor_memory_acquired {
            return Err(Error::AccessDenied);
        }

        let mut available = Vec::new();
        for ram in self.ranges(RangeKind::Ram) {
            let mut cursor = ram.start();
            for reserved in self.ranges(RangeKind::Reserved) {
                if !ram.overlaps(reserved) {
                    continue;
                }
                if cursor < reserved.start() {
                    available.push(PhysAddrRange::new(cursor, reserved.start())?);
                }
                cursor = reserved.end();
            }
            if cursor < ram.end() {
                available.push(PhysAddrRange::new(cursor, ram.end())?);
            }
        }

        if available.is_empty() {
            return Err(Error::NotEnoughResources);
        }
        self.supervisor_memory_acquired = true;
        Ok(SupervisorMemory::new(available))
    }

    /// Returns a typed handle to a registered MMIO window.
    ///
    /// `T` fixes the width of every access through the returned handle and must
    /// match the device's register layout. The range must be non-reserved and
    /// must not overlap a window returned earlier.
    /// Returns [`Error::AccessDenied`] when these requirements are not met.
    pub fn acquire_mmio<T: MmioValue>(&mut self, range: PhysAddrRange) -> Result<MmioRegion<T>> {
        if !self
            .ranges(RangeKind::Mmio)
            .any(|registered| registered.contains(range))
            || self
                .ranges(RangeKind::Reserved)
                .any(|reserved| reserved.overlaps(range))
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

    fn ranges(&self, kind: RangeKind) -> impl Iterator<Item = PhysAddrRange> + '_ {
        self.ranges
            .iter()
            .filter(move |registered| registered.kind == kind)
            .map(|registered| registered.range)
    }
}

fn normalize(
    ram: impl IntoIterator<Item = PhysAddrRange>,
    reserved: impl IntoIterator<Item = PhysAddrRange>,
    mmio: impl IntoIterator<Item = PhysAddrRange>,
) -> Result<Vec<RegisteredRange>> {
    let descriptions: Vec<_> = ram
        .into_iter()
        .map(|range| RegisteredRange::new(range, RangeKind::Ram))
        .chain(
            reserved
                .into_iter()
                .map(|range| RegisteredRange::new(range, RangeKind::Reserved)),
        )
        .chain(
            mmio.into_iter()
                .map(|range| RegisteredRange::new(range, RangeKind::Mmio)),
        )
        .collect();
    let mut normalized = Vec::new();

    for kind in [RangeKind::Ram, RangeKind::Reserved, RangeKind::Mmio] {
        let mut same_kind: Vec<_> = descriptions
            .iter()
            .copied()
            .filter(|registered| registered.kind == kind)
            .collect();
        same_kind.sort_unstable_by_key(|registered| registered.range.start());

        for registered in same_kind {
            let Some(previous) = normalized.last_mut() else {
                normalized.push(registered);
                continue;
            };
            if previous.kind != kind {
                normalized.push(registered);
            } else if previous.range.overlaps(registered.range) {
                return Err(Error::InvalidArgs);
            } else if previous.range.adjacent(registered.range) {
                previous.range = previous.range.join(registered.range);
            } else {
                normalized.push(registered);
            }
        }
    }

    normalized.sort_unstable_by_key(|registered| registered.range.start());
    for (index, first) in normalized.iter().copied().enumerate() {
        for second in normalized[index + 1..].iter().copied() {
            if second.range.start() >= first.range.end() {
                break;
            }
            let allowed_reserved_range = match (first.kind, second.kind) {
                (RangeKind::Reserved, RangeKind::Ram | RangeKind::Mmio) => {
                    second.range.contains(first.range)
                }
                (RangeKind::Ram | RangeKind::Mmio, RangeKind::Reserved) => {
                    first.range.contains(second.range)
                }
                _ => false,
            };
            if !allowed_reserved_range {
                return Err(Error::InvalidArgs);
            }
        }
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{PhysAddr, ReadMemory, WriteMemory};

    fn range(start: usize, len: usize) -> PhysAddrRange {
        PhysAddrRange::from_start_len(PhysAddr::new(start), len).unwrap()
    }

    fn registry(
        ram: &[PhysAddrRange],
        reserved: &[PhysAddrRange],
        mmio: &[PhysAddrRange],
    ) -> MemoryRegistry {
        MemoryRegistry {
            ranges: normalize(
                ram.iter().copied(),
                reserved.iter().copied(),
                mmio.iter().copied(),
            )
            .unwrap(),
            acquired_mmio: Vec::new(),
            supervisor_memory_acquired: false,
        }
    }

    fn can_read(memory: &SupervisorMemory, range: PhysAddrRange) -> bool {
        let mut output = alloc::vec![0; range.size()];
        memory.read(range.start(), &mut output).is_ok()
    }

    #[test]
    fn rejects_conflicting_overlaps() {
        let ram = range(0x1000, 0x1000);
        let overlapping_ram = range(0x1800, 0x1000);
        assert_eq!(
            normalize([ram, overlapping_ram], [], []).unwrap_err(),
            Error::InvalidArgs
        );

        let mmio = range(0x1800, 0x100);
        assert_eq!(
            normalize([ram], [], [mmio]).unwrap_err(),
            Error::InvalidArgs
        );
    }

    #[test]
    fn reserved_range_splits_supervisor_memory() {
        let mut backing = [0u8; 0x1000];
        let base = backing.as_mut_ptr() as usize;
        let mut registry = registry(&[range(base, 0x1000)], &[range(base + 0x400, 0x200)], &[]);
        let mut memory = registry.acquire_supervisor_memory().unwrap();

        assert!(can_read(&memory, range(base, 0x400)));
        assert_eq!(
            memory.read(PhysAddr::new(base + 0x400), &mut [0; 1]),
            Err(Error::InvalidArgs)
        );
        assert_eq!(memory.write(PhysAddr::new(base + 0x600), &[1]), Ok(()));
        assert_eq!(
            registry.acquire_supervisor_memory().err(),
            Some(Error::AccessDenied)
        );
    }

    #[test]
    fn one_access_cannot_cross_ram_banks_or_holes() {
        let mut backing = [0u8; 0x300];
        let base = backing.as_mut_ptr() as usize;
        let mut registry = registry(&[range(base, 0x100), range(base + 0x200, 0x100)], &[], &[]);
        let memory = registry.acquire_supervisor_memory().unwrap();

        assert_eq!(
            memory.read(PhysAddr::new(base + 0x80), &mut [0; 0x80]),
            Ok(())
        );
        assert_eq!(
            memory.read(PhysAddr::new(base + 0x80), &mut [0; 0x180]),
            Err(Error::InvalidArgs)
        );
    }

    #[test]
    fn mmio_is_issued_once_unless_explicitly_shared() {
        let mut registry = registry(&[], &[], &[range(0x2000, 0x100)]);
        let mmio = registry.acquire_mmio::<u32>(range(0x2040, 0x20)).unwrap();
        let _shared = mmio.share();
        let _narrow = mmio.subregion(4, 4).unwrap();

        assert_eq!(
            registry.acquire_mmio::<u32>(range(0x2050, 0x20)).err(),
            Some(Error::AccessDenied)
        );
        assert!(registry.acquire_mmio::<u8>(range(0x2080, 0x20)).is_ok());
    }

    #[test]
    fn adjacent_ranges_are_combined_but_holes_are_preserved() {
        let mut backing = [0u8; 0x400];
        let base = backing.as_mut_ptr() as usize;
        let mut registry = registry(
            &[
                range(base, 0x100),
                range(base + 0x100, 0x100),
                range(base + 0x300, 0x100),
            ],
            &[],
            &[],
        );
        let memory = registry.acquire_supervisor_memory().unwrap();

        assert_eq!(
            memory.read(PhysAddr::new(base + 0x80), &mut [0; 0x100]),
            Ok(())
        );
        assert_eq!(
            memory.read(PhysAddr::new(base + 0x180), &mut [0; 0x200]),
            Err(Error::InvalidArgs)
        );
    }

    #[test]
    fn reserved_mmio_cannot_be_acquired() {
        let mut registry = registry(&[], &[range(0x2040, 0x20)], &[range(0x2000, 0x100)]);

        assert_eq!(
            registry.acquire_mmio::<u8>(range(0x2030, 0x20)).err(),
            Some(Error::AccessDenied)
        );
        assert!(registry.acquire_mmio::<u8>(range(0x2000, 0x40)).is_ok());
        assert!(registry.acquire_mmio::<u8>(range(0x2060, 0xa0)).is_ok());
    }

    #[test]
    fn input_order_does_not_change_normalization() {
        let ram = [range(0x1000, 0x200), range(0x1200, 0x200)];
        let reserved = [range(0x1100, 0x200)];
        let forward = normalize(ram, reserved, []).unwrap();
        let reverse = normalize([ram[1], ram[0]], reserved, []).unwrap();

        assert_eq!(forward, reverse);
    }

    #[test]
    fn reserved_range_can_span_adjacent_ram_inputs() {
        let mut backing = [0u8; 0x200];
        let base = backing.as_mut_ptr() as usize;
        let mut registry = registry(
            &[range(base, 0x100), range(base + 0x100, 0x100)],
            &[range(base + 0x80, 0x100)],
            &[],
        );
        let memory = registry.acquire_supervisor_memory().unwrap();

        assert!(can_read(&memory, range(base, 0x80)));
        assert_eq!(
            memory.read(PhysAddr::new(base + 0x80), &mut [0; 0x100]),
            Err(Error::InvalidArgs)
        );
        assert!(can_read(&memory, range(base + 0x180, 0x80)));
    }

    #[test]
    fn acquiring_missing_supervisor_memory_reports_resource_exhaustion() {
        let mut registry = registry(&[], &[], &[]);
        assert_eq!(
            registry.acquire_supervisor_memory().err(),
            Some(Error::NotEnoughResources)
        );
    }
}
