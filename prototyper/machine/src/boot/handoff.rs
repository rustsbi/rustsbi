//! Final device-tree handoff storage.

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::ops::Range;
use core::sync::atomic::{AtomicBool, Ordering};

use alloc::boxed::Box;
use alloc::vec::Vec;
use dtoolkit::fdt::Fdt;
use dtoolkit::memreserve::MemoryReservation;
use dtoolkit::model::DeviceTree;

use super::BootDtb;
use super::dtb::BootDtbStorage;
use crate::config::BOOT_DTB_MAX_SIZE;

#[repr(C, align(8))]
struct Buffer(UnsafeCell<[MaybeUninit<u8>; BOOT_DTB_MAX_SIZE]>);

// SAFETY: terminal transfer initializes this buffer once and then gives up
// every Rust access path before it publishes the address.
unsafe impl Sync for Buffer {}

static CLAIMED: AtomicBool = AtomicBool::new(false);

#[used]
#[unsafe(link_section = ".handoff")]
static BUFFER: Buffer = Buffer(UnsafeCell::new(
    [const { MaybeUninit::uninit() }; BOOT_DTB_MAX_SIZE],
));

pub(super) fn device_tree(dtb: BootDtb) -> Option<usize> {
    if CLAIMED.swap(true, Ordering::AcqRel) {
        return None;
    }
    let source = match dtb.storage {
        BootDtbStorage::Entry(bytes) => &*bytes,
        BootDtbStorage::Encoded(bytes) => Box::leak(bytes.into_boxed_slice()),
    };
    let bytes = encode(source, crate::pmp::machine_image_range()?)?;
    if bytes.is_empty() || bytes.len() > BOOT_DTB_MAX_SIZE {
        return None;
    }
    let destination = BUFFER.0.get().cast::<MaybeUninit<u8>>();
    // SAFETY: the successful claim owns the complete initialized prefix.
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), destination.cast::<u8>(), bytes.len())
    };
    Some(destination as usize)
}

#[cfg(test)]
pub(super) fn encode_for_test(bytes: &[u8], image: Range<usize>) -> Option<Vec<u8>> {
    encode(bytes, image)
}

fn encode(bytes: &[u8], image: Range<usize>) -> Option<Vec<u8>> {
    let size = image
        .end
        .checked_sub(image.start)
        .filter(|size| *size != 0)?;
    let reservation =
        MemoryReservation::new(u64::try_from(image.start).ok()?, u64::try_from(size).ok()?);
    let fdt = Fdt::new(bytes).ok()?;
    let mut tree = DeviceTree::from_fdt(&fdt);
    if !tree.memory_reservations.contains(&reservation) {
        tree.memory_reservations.push(reservation);
        tree.memory_reservations.sort_unstable();
    }
    let encoded = tree.to_dtb();
    (encoded.len() <= BOOT_DTB_MAX_SIZE && Fdt::new(&encoded).is_ok()).then_some(encoded)
}
