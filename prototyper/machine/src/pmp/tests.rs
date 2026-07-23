use alloc::vec::Vec;

use super::*;

fn capability(entries: usize) -> Capability {
    Capability::new(entries, 4, usize::MAX).unwrap()
}

fn configuration(range: core::ops::Range<usize>, permissions: Permissions) -> Configuration {
    let mut configuration = Configuration::empty();
    configuration.grant(range, permissions).unwrap();
    configuration
}

fn protected(image: Image) -> (Vec<Entry>, usize) {
    let Image::Protected {
        entries,
        deny_count,
    } = image
    else {
        panic!("expected protected image")
    };
    (entries, deny_count)
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
    let grants = [Grant {
        region: Region::new(0x1000, 0x2c00).unwrap(),
        permissions: Permissions::READ,
    }];
    let (entries, deny_count) = protected(compile(&[], &grants, capability(8), false).unwrap());
    assert_eq!(deny_count, 0);
    let decoded: Vec<_> = entries.iter().copied().map(decode).collect();
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
    assert!(
        entries
            .iter()
            .all(|entry| entry.permissions == Permissions::READ)
    );
}

#[test]
fn machine_floor_precedes_explicit_grants() {
    let image = Region::new(0x8000_0000, 0x8000_1000).unwrap();
    let configuration = configuration(
        0x8000_0000..0x8800_0000,
        Permissions::READ | Permissions::WRITE | Permissions::EXECUTE,
    );
    let (entries, deny_count) = protected(
        compile_machine_policy(image, &[], &configuration, capability(8), false).unwrap(),
    );
    assert_eq!(decode(entries[0]), image);
    assert!(
        entries[..deny_count]
            .iter()
            .all(|entry| entry.permissions.is_empty())
    );
    assert!(
        entries[deny_count..]
            .iter()
            .all(|entry| !entry.permissions.is_empty())
    );
}

#[test]
fn unmatched_addresses_have_no_compatibility_allow_entry() {
    let configuration = configuration(0x8000_0000..0x8000_1000, Permissions::READ);
    let (entries, deny_count) =
        protected(compile(&[], &configuration.grants, capability(4), false).unwrap());
    assert_eq!(deny_count, 0);
    assert_eq!(entries.len(), 1);
    assert_eq!(
        decode(entries[0]),
        Region::new(0x8000_0000, 0x8000_1000).unwrap()
    );
    assert_eq!(entries[0].permissions, Permissions::READ);
}

#[test]
fn configuration_rejects_overlaps_and_write_without_read() {
    let mut configuration = Configuration::empty();
    assert_eq!(
        configuration.grant(0x1000..0x2000, Permissions::WRITE),
        Err(PmpError::InvalidRegion)
    );
    configuration
        .grant(0x1000..0x2000, Permissions::READ)
        .unwrap();
    assert_eq!(
        configuration.grant(0x1800..0x2800, Permissions::READ),
        Err(PmpError::InvalidRegion)
    );
}

#[test]
fn namespaced_macro_builds_semantic_permissions() {
    let ram = 0x8000_0000..0x8100_0000;
    let uart = 0x1000_0000..0x1000_1000;
    let configuration = crate::pmp::config! {
        ram => [read, write, execute];
        uart => [read, write];
    }
    .unwrap();
    assert_eq!(configuration.grants.len(), 2);
    assert_eq!(configuration.grants[0].region.start, 0x8000_0000);
    assert_eq!(configuration.grants[0].permissions, Permissions::all());
    assert_eq!(
        configuration.grants[1].permissions,
        Permissions::READ | Permissions::WRITE
    );
}

#[test]
fn insufficient_capacity_never_coarsens_policy() {
    let configuration = configuration(0x1000..0x2c00, Permissions::READ);
    assert_eq!(
        compile(&[], &configuration.grants, capability(2), false),
        Err(PmpError::InsufficientEntries)
    );
}

#[test]
fn hardware_granularity_can_make_an_exact_region_unrepresentable() {
    let capability = Capability::new(8, 16, usize::MAX).unwrap();
    let configuration = configuration(0x1004..0x1010, Permissions::READ);
    assert_eq!(
        compile(&[], &configuration.grants, capability, false),
        Err(PmpError::Unrepresentable)
    );
}

#[test]
fn no_pmp_requires_an_explicit_trusted_target() {
    let no_pmp = Capability::new(0, 4, 0).unwrap();
    let configuration = Configuration::empty();
    assert_eq!(
        compile(&[], &configuration.grants, no_pmp, false),
        Err(PmpError::PmpRequired)
    );
    assert_eq!(
        compile(&[], &configuration.grants, no_pmp, true),
        Ok(Image::TrustedWithoutPmp)
    );
}

#[test]
fn every_small_aligned_interval_round_trips_exactly() {
    for start in (0x1000..0x1100).step_by(4) {
        for end in ((start + 4)..=0x1100).step_by(4) {
            let configuration = configuration(start..end, Permissions::READ);
            let (entries, _) =
                protected(compile(&[], &configuration.grants, capability(64), false).unwrap());
            let blocks: Vec<_> = entries.iter().copied().map(decode).collect();
            assert_eq!(blocks.first().unwrap().start, start);
            assert_eq!(blocks.last().unwrap().end, end);
            assert!(blocks.windows(2).all(|pair| pair[0].end == pair[1].start));
        }
    }
}
