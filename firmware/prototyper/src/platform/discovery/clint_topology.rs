//! Pure decoding of CLINT per-hart interrupt wiring.
//! Physical slots count all pairs, including disabled CPUs.
//! Only enabled harts appear in the resulting software routes.

use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ClintHartSlot {
    pub(crate) hart_id: usize,
    pub(crate) slot: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TopologyError {
    Malformed,
    UnknownPhandle,
    DuplicateHart,
    MissingHart,
}

/// Resolves each phandle to (global hart ID, whether runtime enabled).
pub(crate) fn decode(
    bytes: &[u8],
    mut resolve: impl FnMut(u32) -> Option<(usize, bool)>,
) -> Result<(usize, Vec<ClintHartSlot>), TopologyError> {
    if bytes.is_empty() || bytes.len() % 16 != 0 {
        return Err(TopologyError::Malformed);
    }
    let mut enabled = Vec::new();
    let mut seen = Vec::new();
    for (slot, pair) in bytes.chunks_exact(16).enumerate() {
        let cell = |index: usize| {
            let start = index * 4;
            u32::from_be_bytes(pair[start..start + 4].try_into().unwrap())
        };
        let (first, msip, second, mtip) = (cell(0), cell(1), cell(2), cell(3));
        if first != second || !((msip == 3 && mtip == 7) || (msip == 7 && mtip == 3)) {
            return Err(TopologyError::Malformed);
        }
        let (hart_id, active) = resolve(first).ok_or(TopologyError::UnknownPhandle)?;
        if seen.contains(&hart_id) {
            return Err(TopologyError::DuplicateHart);
        }
        seen.push(hart_id);
        if active {
            enabled.push(ClintHartSlot { hart_id, slot });
        }
    }
    Ok((bytes.len() / 16, enabled))
}

/// Builds a dense index-by-enabled-hart route table. A physical slot is
/// local to its controller, and must fall inside the paired interrupt list.
pub(crate) fn build_routes<'a>(
    hart_count: usize,
    controllers: impl IntoIterator<Item = (&'a [ClintHartSlot], usize)>,
    mut index_of: impl FnMut(usize) -> Option<usize>,
) -> Result<Vec<(usize, usize)>, TopologyError> {
    let mut routes = alloc::vec![None; hart_count];
    for (device, (slots, physical_count)) in controllers.into_iter().enumerate() {
        for slot in slots {
            if slot.slot >= physical_count {
                return Err(TopologyError::Malformed);
            }
            let index = index_of(slot.hart_id).ok_or(TopologyError::UnknownPhandle)?;
            let target = routes.get_mut(index).ok_or(TopologyError::Malformed)?;
            if target.replace((device, slot.slot)).is_some() {
                return Err(TopologyError::DuplicateHart);
            }
        }
    }
    routes
        .into_iter()
        .map(|r| r.ok_or(TopologyError::MissingHart))
        .collect()
}

/// Tests only the physical memory intervals; zero-length/overlap is rejected.
pub(crate) fn disjoint_ranges(ranges: impl IntoIterator<Item = (usize, usize)>) -> bool {
    let mut prior: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        if start >= end || prior.iter().any(|(lo, hi)| start < *hi && *lo < end) {
            return false;
        }
        prior.push((start, end));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bytes(cells: &[u32]) -> Vec<u8> {
        cells.iter().flat_map(|n| n.to_be_bytes()).collect()
    }
    fn resolver(ph: u32) -> Option<(usize, bool)> {
        match ph {
            10 => Some((7, true)),
            11 => Some((9, false)),
            12 => Some((42, true)),
            _ => None,
        }
    }
    #[test]
    fn physical_slots_preserve_disabled_and_nonidentity_harts() {
        let (count, slots) = decode(
            &bytes(&[12, 3, 12, 7, 11, 3, 11, 7, 10, 3, 10, 7]),
            resolver,
        )
        .unwrap();
        assert_eq!(count, 3);
        assert_eq!(
            slots,
            vec![
                ClintHartSlot {
                    hart_id: 42,
                    slot: 0
                },
                ClintHartSlot {
                    hart_id: 7,
                    slot: 2
                },
            ]
        );
    }
    #[test]
    fn disjoint_controllers_reset_local_slots() {
        let (_, a) = decode(&bytes(&[10, 3, 10, 7]), resolver).unwrap();
        let (_, b) = decode(&bytes(&[12, 3, 12, 7]), resolver).unwrap();
        assert_eq!(a[0].slot, 0);
        assert_eq!(b[0].slot, 0);
    }
    #[test]
    fn reverse_irq_order_is_valid() {
        assert_eq!(
            decode(&bytes(&[10, 7, 10, 3]), resolver).unwrap().1[0].slot,
            0
        );
    }
    #[test]
    fn routes_all_enabled_harts_on_disjoint_devices() {
        let a = [ClintHartSlot {
            hart_id: 7,
            slot: 2,
        }];
        let b = [ClintHartSlot {
            hart_id: 42,
            slot: 0,
        }];
        let table = build_routes(2, [(&a[..], 3), (&b[..], 1)], |raw| match raw {
            7 => Some(0),
            42 => Some(1),
            _ => None,
        })
        .unwrap();
        assert_eq!(table, vec![(0, 2), (1, 0)]);
    }
    #[test]
    fn rejects_uncovered_duplicate_and_invalid_slot() {
        let a = [ClintHartSlot {
            hart_id: 7,
            slot: 0,
        }];
        let b = [ClintHartSlot {
            hart_id: 7,
            slot: 1,
        }];
        let idx = |raw| (raw == 7).then_some(0);
        assert_eq!(
            build_routes(2, [(&a[..], 1)], idx),
            Err(TopologyError::MissingHart)
        );
        assert_eq!(
            build_routes(1, [(&a[..], 1), (&b[..], 2)], idx),
            Err(TopologyError::DuplicateHart)
        );
        assert_eq!(
            build_routes(1, [(&b[..], 1)], idx),
            Err(TopologyError::Malformed)
        );
        assert_eq!(
            build_routes(1, [(&a[..], 1)], |_| None),
            Err(TopologyError::UnknownPhandle)
        );
    }
    #[test]
    fn rejects_overlapping_mmio_and_allows_adjacent() {
        assert!(disjoint_ranges([
            (0x2000000, 0x2010000),
            (0x3000000, 0x3010000)
        ]));
        assert!(disjoint_ranges([
            (0x2000000, 0x2010000),
            (0x2010000, 0x2020000)
        ]));
        assert!(!disjoint_ranges([
            (0x2000000, 0x2010000),
            (0x2008000, 0x2018000)
        ]));
        assert!(!disjoint_ranges([(0x2000000, 0x2000000)]));
    }
    #[test]
    fn rejects_malformed_unknown_and_duplicate() {
        for cells in [
            vec![10, 3, 12, 7],
            vec![10, 3, 10, 9],
            vec![10, 3, 10, 7, 10, 3, 10, 7],
        ] {
            assert!(decode(&bytes(&cells), resolver).is_err());
        }
        assert_eq!(
            decode(&bytes(&[99, 3, 99, 7]), resolver),
            Err(TopologyError::UnknownPhandle)
        );
        assert_eq!(
            decode(&bytes(&[10, 3, 10]), resolver),
            Err(TopologyError::Malformed)
        );
        assert_eq!(decode(&[], resolver), Err(TopologyError::Malformed));
    }
}
