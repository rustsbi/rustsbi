use alloc::vec::Vec;
use dtoolkit::fdt::Fdt;
use dtoolkit::model::DeviceTree;

use super::*;

#[repr(C, align(8))]
struct AlignedDtb([u8; DTB_HEADER_SIZE]);

static SOURCE_DTB: AlignedDtb = {
    let mut bytes = [0; DTB_HEADER_SIZE];
    bytes[0] = 0xd0;
    bytes[1] = 0x0d;
    bytes[2] = 0xfe;
    bytes[3] = 0xed;
    bytes[7] = DTB_HEADER_SIZE as u8;
    AlignedDtb(bytes)
};

fn header(size: u32) -> [u8; 8] {
    let mut header = [0; 8];
    header[..4].copy_from_slice(&DTB_MAGIC.to_be_bytes());
    header[4..].copy_from_slice(&size.to_be_bytes());
    header
}

#[test]
fn validates_an_exact_bounded_envelope() {
    assert_eq!(validate_envelope(0x8800_0000, header(4096), 4096), Ok(4096));
}

#[test]
fn rejects_invalid_addresses() {
    assert_eq!(
        validate_envelope(0, header(4096), 4096),
        Err(BootDtbImportError::NullAddress)
    );
    assert_eq!(
        validate_envelope(0x8800_0004, header(4096), 4096),
        Err(BootDtbImportError::MisalignedAddress)
    );
    assert_eq!(
        validate_envelope(usize::MAX - 31, header(40), 4096),
        Err(BootDtbImportError::AddressOverflow)
    );
}

#[test]
fn rejects_bad_or_over_budget_sizes() {
    let mut bad_magic = header(4096);
    bad_magic[0] ^= 1;
    assert_eq!(
        validate_envelope(0x8800_0000, bad_magic, 4096),
        Err(BootDtbImportError::BadMagic)
    );
    assert_eq!(
        validate_envelope(0x8800_0000, header(39), 4096),
        Err(BootDtbImportError::InvalidSize)
    );
    assert_eq!(
        validate_envelope(0x8800_0000, header(4097), 4096),
        Err(BootDtbImportError::SizeLimitExceeded)
    );
}

#[test]
fn copies_provider_bytes_into_machine_owned_storage_once() {
    let address = SOURCE_DTB.0.as_ptr() as usize;
    // SAFETY: the source is a stable, aligned static containing exactly the
    // complete header-declared envelope, and this is the only import test.
    let dtb = unsafe { copy_from_entry(address) }.unwrap();
    assert_eq!(dtb.as_bytes(), SOURCE_DTB.0);

    // SAFETY: this deliberate second call verifies that the unique claim
    // rejects access before reading the provider pointer again.
    assert_eq!(
        unsafe { copy_from_entry(address) }.err(),
        Some(BootDtbImportError::AlreadyImported)
    );
}

#[test]
fn encoded_replacement_is_transactional_and_revalidated() {
    let initial = DeviceTree::new().to_dtb();
    let mut boot = BootDtb {
        storage: BootDtbStorage::Encoded(initial.clone()),
    };

    let mut malformed = initial.clone();
    malformed[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(
        boot.replace_encoded(malformed),
        Err(BootDtbImportError::InvalidSize)
    );
    assert_eq!(boot.as_bytes(), initial);

    let replacement = DeviceTree::new().to_dtb();
    boot.replace_encoded(replacement.clone()).unwrap();
    assert_eq!(boot.as_bytes(), replacement);
    assert!(Fdt::new(boot.as_bytes()).is_ok());
}

#[test]
fn terminal_encoding_adds_the_machine_image_reservation_once() {
    let initial = DeviceTree::new().to_dtb();
    let encoded = encode_for_test(&initial, 0x8000_0000..0x8012_3000).unwrap();
    let encoded = encode_for_test(&encoded, 0x8000_0000..0x8012_3000).unwrap();
    let reservations = Fdt::new(&encoded)
        .unwrap()
        .memory_reservations()
        .collect::<Vec<_>>();

    assert_eq!(reservations.len(), 1);
    assert_eq!(reservations[0].address(), 0x8000_0000);
    assert_eq!(reservations[0].size(), 0x12_3000);
}

#[test]
fn machine_resource_claims_are_disjoint_and_transactional() {
    let dtb = BootDtb {
        storage: BootDtbStorage::Encoded(DeviceTree::new().to_dtb()),
    };
    let next_stage = NextStage {
        entry: 0x8020_0000,
        opaque: 0,
        mode: NextMode::Supervisor,
    };
    let mut boot = BootInfo::new(dtb, next_stage, 0);

    assert!(boot.claim_machine_range(0x2000..0x3000));
    assert!(!boot.claim_machine_range(0x2800..0x3800));
    assert_eq!(boot.machine_ranges.len(), 1);
    assert_eq!(boot.machine_ranges[0], 0x2000..0x3000);
}

#[test]
fn supervisor_next_stage_validates_entry_without_exposing_mode_bits() {
    assert!(NextStage::supervisor(0x8020_0000, 7).is_ok());
    assert_eq!(
        NextStage::supervisor(0x8020_0001, 7).err(),
        Some(crate::HartError::InvalidAddress)
    );
    assert_eq!(
        NextStage::supervisor(1, 7).err(),
        Some(crate::HartError::InvalidAddress)
    );
}

#[test]
fn warm_wait_enables_only_the_selected_notification_cause() {
    assert_eq!(
        crate::hart::Notification::Software.machine_interrupt_bit(),
        1 << 3
    );
    assert_eq!(
        crate::hart::Notification::External.machine_interrupt_bit(),
        1 << 11
    );
}
