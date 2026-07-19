//! Owned device-tree storage, validation, and handoff encoding.

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::ops::Range;
use core::sync::atomic::{AtomicBool, Ordering};

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
use alloc::boxed::Box;
use alloc::vec::Vec;
use dtoolkit::fdt::Fdt;
use dtoolkit::memreserve::MemoryReservation;
use dtoolkit::model::DeviceTree;

use crate::config::BOOT_DTB_MAX_SIZE;

pub(crate) const DTB_MAGIC: u32 = 0xd00d_feed;
pub(crate) const DTB_HEADER_SIZE: usize = 40;

#[repr(C, align(8))]
struct BootDtbBuffer(UnsafeCell<[MaybeUninit<u8>; BOOT_DTB_MAX_SIZE]>);

#[repr(C, align(8))]
struct HandoffDtbBuffer(UnsafeCell<[MaybeUninit<u8>; BOOT_DTB_MAX_SIZE]>);

// SAFETY: the buffer is reachable only through `copy_from_entry`. Its atomic
// one-time claim establishes unique initialization, and `BootDtb` then owns the
// sole mutable slice for the rest of the boot phase.
unsafe impl Sync for BootDtbBuffer {}

// SAFETY: the terminal handoff claims this buffer exactly once, initializes
// the complete copied prefix, and publishes its address only after machine
// code has relinquished every Rust access path to the bytes.
unsafe impl Sync for HandoffDtbBuffer {}

static BOOT_DTB_CLAIMED: AtomicBool = AtomicBool::new(false);
static HANDOFF_DTB_CLAIMED: AtomicBool = AtomicBool::new(false);
static BOOT_DTB_BUFFER: BootDtbBuffer = BootDtbBuffer(UnsafeCell::new(
    [const { MaybeUninit::uninit() }; BOOT_DTB_MAX_SIZE],
));
#[used]
#[unsafe(link_section = ".handoff")]
static HANDOFF_DTB_BUFFER: HandoffDtbBuffer = HandoffDtbBuffer(UnsafeCell::new(
    [const { MaybeUninit::uninit() }; BOOT_DTB_MAX_SIZE],
));

/// An owned device tree supplied to safe firmware policy during cold boot.
///
/// The bytes reside in firmware-controlled storage. This capability exposes no
/// physical address and is consumed by the eventual next-stage handoff.
pub struct BootDtb {
    pub(super) storage: BootDtbStorage,
}

pub(super) enum BootDtbStorage {
    Entry(&'static mut [u8]),
    Encoded(Vec<u8>),
}

impl BootDtb {
    /// Borrows the complete validated flattened device-tree blob.
    pub fn as_bytes(&self) -> &[u8] {
        match &self.storage {
            BootDtbStorage::Entry(bytes) => bytes,
            BootDtbStorage::Encoded(bytes) => bytes,
        }
    }

    /// Mutably borrows the owned blob for the narrow platform DT adapter.
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        match &mut self.storage {
            BootDtbStorage::Entry(bytes) => bytes,
            BootDtbStorage::Encoded(bytes) => bytes,
        }
    }

    /// Replaces the backing allocation after upper policy has re-encoded the
    /// supervisor-visible tree.
    ///
    /// The complete encoded blob is independently parsed before ownership is
    /// accepted. A failure leaves the current boot blob unchanged.
    #[doc(hidden)]
    pub fn replace_encoded(&mut self, bytes: Vec<u8>) -> Result<(), BootDtbImportError> {
        if bytes.len() > BOOT_DTB_MAX_SIZE {
            return Err(BootDtbImportError::SizeLimitExceeded);
        }
        validate_owned_blob(&bytes)?;
        self.storage = BootDtbStorage::Encoded(bytes);
        Ok(())
    }

    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    pub(super) fn into_handoff_address(self) -> Result<usize, HandoffDtbError> {
        if HANDOFF_DTB_CLAIMED.swap(true, Ordering::AcqRel) {
            return Err(HandoffDtbError::AlreadyPublished);
        }
        let source = match self.storage {
            BootDtbStorage::Entry(bytes) => &*bytes,
            BootDtbStorage::Encoded(bytes) => Box::leak(bytes.into_boxed_slice()),
        };
        let machine_image =
            crate::pmp::machine_image_range().ok_or(HandoffDtbError::MachineImageUnavailable)?;
        let encoded = encode_handoff_dtb(source, machine_image)?;
        let bytes = encoded.as_slice();
        if bytes.is_empty() || bytes.len() > BOOT_DTB_MAX_SIZE {
            return Err(HandoffDtbError::InvalidSize);
        }
        let destination = HANDOFF_DTB_BUFFER.0.get().cast::<MaybeUninit<u8>>();
        // SAFETY: the one-time claim grants unique initialization of the
        // complete destination prefix. Source and handoff sections are
        // disjoint, and no Rust access occurs after the address is published.
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), destination.cast::<u8>(), bytes.len())
        };
        Ok(destination as usize)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum HandoffDtbError {
    AlreadyPublished,
    InvalidSize,
    InvalidTree,
    MachineImageUnavailable,
    ReservationOutOfRange,
}

pub(super) fn encode_handoff_dtb(
    bytes: &[u8],
    machine_image: Range<usize>,
) -> Result<Vec<u8>, HandoffDtbError> {
    let size = machine_image
        .end
        .checked_sub(machine_image.start)
        .filter(|size| *size != 0)
        .ok_or(HandoffDtbError::ReservationOutOfRange)?;
    let address =
        u64::try_from(machine_image.start).map_err(|_| HandoffDtbError::ReservationOutOfRange)?;
    let size = u64::try_from(size).map_err(|_| HandoffDtbError::ReservationOutOfRange)?;
    let fdt = Fdt::new(bytes).map_err(|_| HandoffDtbError::InvalidTree)?;
    let mut tree = DeviceTree::from_fdt(&fdt);
    let reservation = MemoryReservation::new(address, size);
    if !tree.memory_reservations.contains(&reservation) {
        tree.memory_reservations.push(reservation);
        tree.memory_reservations.sort_unstable();
    }

    // The owned input and configured output cap are far below every `u32`
    // length used by the encoder. The only added record has fixed size.
    let encoded = tree.to_dtb();
    if encoded.len() > BOOT_DTB_MAX_SIZE || Fdt::new(&encoded).is_err() {
        return Err(HandoffDtbError::InvalidTree);
    }
    Ok(encoded)
}

/// Rejection reasons at the boot-DTB import boundary.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootDtbImportError {
    /// Another caller already consumed the unique boot-DTB storage.
    AlreadyImported,
    /// The source address is null.
    NullAddress,
    /// The source address does not meet the boot ABI alignment.
    MisalignedAddress,
    /// The copied header magic does not identify a flattened device tree.
    BadMagic,
    /// The header-declared size cannot contain a complete DTB header.
    InvalidSize,
    /// The header-declared size exceeds configured firmware-owned storage.
    SizeLimitExceeded,
    /// The source address range wraps around XLEN.
    AddressOverflow,
    /// The complete owned blob is not a well-formed flattened device tree.
    InvalidStructure,
}

pub(crate) fn validate_envelope(
    address: usize,
    header: [u8; 8],
    max_size: usize,
) -> Result<usize, BootDtbImportError> {
    validate_header_address(address)?;

    let magic = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
    if magic != DTB_MAGIC {
        return Err(BootDtbImportError::BadMagic);
    }

    let size = u32::from_be_bytes([header[4], header[5], header[6], header[7]]) as usize;
    if size < DTB_HEADER_SIZE {
        return Err(BootDtbImportError::InvalidSize);
    }
    if size > max_size {
        return Err(BootDtbImportError::SizeLimitExceeded);
    }
    if address.checked_add(size).is_none() {
        return Err(BootDtbImportError::AddressOverflow);
    }
    Ok(size)
}

fn validate_owned_blob(bytes: &[u8]) -> Result<(), BootDtbImportError> {
    if bytes.len() < DTB_HEADER_SIZE {
        return Err(BootDtbImportError::InvalidSize);
    }
    let declared = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    if declared != bytes.len() {
        return Err(BootDtbImportError::InvalidSize);
    }
    Fdt::new(bytes).map_err(|_| BootDtbImportError::InvalidStructure)?;
    Ok(())
}

fn validate_header_address(address: usize) -> Result<(), BootDtbImportError> {
    if address == 0 {
        return Err(BootDtbImportError::NullAddress);
    }
    if !address.is_multiple_of(8) {
        return Err(BootDtbImportError::MisalignedAddress);
    }
    if address.checked_add(8).is_none() {
        return Err(BootDtbImportError::AddressOverflow);
    }
    Ok(())
}

/// Copies the complete DTB without constructing a reference over provider
/// memory.
///
/// # Safety
///
/// The caller establishes the previous-stage readable-memory envelope stated
/// by the raw cold-entry contract.
pub(crate) unsafe fn copy_from_entry(address: usize) -> Result<BootDtb, BootDtbImportError> {
    if BOOT_DTB_CLAIMED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(BootDtbImportError::AlreadyImported);
    }

    validate_header_address(address)?;
    let source = address as *const u8;
    let mut header = [0; 8];
    for (offset, byte) in header.iter_mut().enumerate() {
        // TODO: A future early-fault protocol may diagnose a previous stage
        // that violates the stable readable-envelope contract. Address checks
        // alone cannot make an inaccessible physical pointer safe to read.
        // SAFETY: the caller guarantees that the fixed header lies in the
        *byte = unsafe { source.add(offset).read_volatile() };
    }
    let size = validate_envelope(address, header, BOOT_DTB_MAX_SIZE)?;

    let destination = BOOT_DTB_BUFFER.0.get().cast::<MaybeUninit<u8>>();
    for offset in 0..size {
        // SAFETY: validation proves `offset < size <= BOOT_DTB_MAX_SIZE`; the
        // unique claim excludes every competing destination access, and the
        // caller's envelope covers the complete source range.
        let byte = unsafe { source.add(offset).read_volatile() };
        unsafe { destination.add(offset).write(MaybeUninit::new(byte)) };
    }

    // SAFETY: the preceding loop initialized exactly `size` bytes within the
    // uniquely claimed static buffer. No other reference to those bytes exists.
    let bytes =
        unsafe { core::slice::from_raw_parts_mut(BOOT_DTB_BUFFER.0.get().cast::<u8>(), size) };
    Ok(BootDtb {
        storage: BootDtbStorage::Entry(bytes),
    })
}
