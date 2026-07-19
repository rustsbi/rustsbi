use alloc::vec::Vec;

use super::*;

fn capability(entries: usize) -> Capability {
    Capability::new(entries, 4, usize::MAX).unwrap()
}

fn protected(image: Image) -> Vec<Entry> {
    let Image::Protected(entries) = image else {
        panic!("expected protected image")
    };
    entries
}

fn decode(entry: Entry) -> Region {
    match entry.mode {
        AddressMode::NaturallyAlignedFourBytes => Region {
            start: entry.address << 2,
            end: (entry.address << 2) + 4,
        },
        AddressMode::NaturallyAlignedPowerOfTwo => {
            let low_ones = entry.address.trailing_ones();
            let size = 1usize << (low_ones + 3);
            let start = (entry.address & !((1usize << low_ones) - 1)) << 2;
            Region {
                start,
                end: start.saturating_add(size),
            }
        }
    }
}

#[test]
fn exact_decomposition_never_enlarges_a_range() {
    let entries = protected(
        compile(
            &[Region::new(0x1000, 0x2c00).unwrap()],
            capability(8),
            false,
        )
        .unwrap(),
    );
    let decoded: Vec<_> = entries[..entries.len() - 1]
        .iter()
        .copied()
        .map(decode)
        .collect();
    assert_eq!(
        decoded,
        [
            Region {
                start: 0x1000,
                end: 0x2000,
            },
            Region {
                start: 0x2000,
                end: 0x2800,
            },
            Region {
                start: 0x2800,
                end: 0x2c00,
            },
        ]
    );
}

#[test]
fn overlapping_and_adjacent_exclusions_merge_before_encoding() {
    let entries = protected(
        compile(
            &[
                Region::new(0x1800, 0x2000).unwrap(),
                Region::new(0x1000, 0x1800).unwrap(),
                Region::new(0x1400, 0x1c00).unwrap(),
            ],
            capability(3),
            false,
        )
        .unwrap(),
    );
    assert_eq!(entries.len(), 2);
    assert_eq!(decode(entries[0]), Region::new(0x1000, 0x2000).unwrap());
}

#[test]
fn broad_compatibility_entry_is_last_and_permissive() {
    let entries = protected(
        compile(
            &[Region::new(0x8000_0000, 0x8000_1000).unwrap()],
            capability(4),
            false,
        )
        .unwrap(),
    );
    let broad = entries.last().unwrap();
    assert_eq!(broad.permissions, Permissions::all());
    assert_eq!(broad.mode, AddressMode::NaturallyAlignedPowerOfTwo);
    assert_eq!(broad.address, usize::MAX);
}

#[test]
fn insufficient_capacity_never_coarsens_policy() {
    assert_eq!(
        compile(
            &[Region::new(0x1000, 0x2c00).unwrap()],
            capability(3),
            false,
        ),
        Err(PmpError::InsufficientEntries)
    );
}

#[test]
fn machine_image_is_mandatory_even_without_device_ranges() {
    let image = Region::new(0x8000_0000, 0x8000_1000).unwrap();
    let entries = protected(compile_machine_policy(image, &[], capability(4), false).unwrap());
    assert_eq!(decode(entries[0]), image);
}

#[test]
fn hardware_granularity_can_make_an_exact_region_unrepresentable() {
    let capability = Capability::new(8, 16, usize::MAX).unwrap();
    assert_eq!(
        compile(&[Region::new(0x1004, 0x1010).unwrap()], capability, false),
        Err(PmpError::Unrepresentable)
    );
}

#[test]
fn no_pmp_requires_an_explicit_trusted_configuration() {
    let no_pmp = Capability::new(0, 4, 0).unwrap();
    assert_eq!(compile(&[], no_pmp, false), Err(PmpError::PmpRequired));
    assert_eq!(compile(&[], no_pmp, true), Ok(Image::TrustedWithoutPmp));
}

#[test]
fn address_range_uses_the_probed_pmpaddr_mask() {
    let capability = Capability::new(4, 4, 0x3ff).unwrap();
    assert!(compile(&[Region::new(0xffc, 0x1000).unwrap()], capability, false).is_ok());
    assert_eq!(
        compile(&[Region::new(0x1000, 0x1004).unwrap()], capability, false),
        Err(PmpError::AddressOutOfRange)
    );
}

#[test]
fn capability_rejects_noncontiguous_pmpaddr_masks() {
    assert_eq!(
        Capability::new(4, 4, 0b1011),
        Err(PmpError::InvalidCapability)
    );
}

#[test]
fn every_small_aligned_interval_round_trips_exactly() {
    for start in (0x1000..0x1100).step_by(4) {
        for end in ((start + 4)..=0x1100).step_by(4) {
            let entries = protected(
                compile(&[Region::new(start, end).unwrap()], capability(64), false).unwrap(),
            );
            let blocks: Vec<_> = entries[..entries.len() - 1]
                .iter()
                .copied()
                .map(decode)
                .collect();
            assert_eq!(blocks.first().unwrap().start, start);
            assert_eq!(blocks.last().unwrap().end, end);
            assert!(blocks.windows(2).all(|pair| pair[0].end == pair[1].start));
        }
    }
}
