//! Bounded volatile access to one permanently claimed ordinary MMIO range.

use core::marker::PhantomData;
use core::mem::{align_of, size_of};
use core::ops::Range;

use alloc::vec::Vec;
use dtoolkit::fdt::{Fdt, FdtNode};
use dtoolkit::{Node, Property};
use spin::Mutex;

use crate::boot::device_tree::{enabled, model, reg_ranges};
use crate::config::TRUSTED_TARGET;

mod sealed {
    pub trait Sealed {}
}

/// A scalar register value supported by [`IoMem`].
///
/// This trait is sealed: MMIO does not accept arbitrary Rust layouts.
pub trait IoValue: sealed::Sealed + Copy {
    #[doc(hidden)]
    unsafe fn read(address: usize) -> Self;

    #[doc(hidden)]
    unsafe fn write(address: usize, value: Self);
}

macro_rules! impl_io_value {
    ($($type:ty),+ $(,)?) => {
        $(
            impl sealed::Sealed for $type {}

            impl IoValue for $type {
                unsafe fn read(address: usize) -> Self {
                    // SAFETY: the caller validated the complete aligned access
                    // inside an exclusively claimed MMIO range.
                    unsafe { (address as *const Self).read_volatile() }
                }

                unsafe fn write(address: usize, value: Self) {
                    // SAFETY: same range and alignment proof as `read`.
                    unsafe { (address as *mut Self).write_volatile(value) }
                }
            }
        )+
    };
}

impl_io_value!(u8, u16, u32);

/// Failure while claiming or accessing an ordinary MMIO range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IoMemError {
    /// The range is empty, wraps, or violates machine-range alignment.
    InvalidRange,
    /// The range overlaps a resource already owned by machine firmware.
    AlreadyClaimed,
    /// The requested region or scalar access lies outside the claimed range.
    OutOfBounds,
    /// The scalar access is not naturally aligned.
    Misaligned,
}

/// Exclusive ownership of one ordinary physical MMIO range.
///
/// The capability is deliberately neither `Clone` nor `Copy` and exposes no
/// physical pointer. Dropping it does not release the permanent machine claim.
pub struct IoMem {
    range: Range<usize>,
    _not_sendable_by_layout_accident: PhantomData<fn() -> ()>,
}

impl IoMem {
    /// Permanently claims an ordinary MMIO range during boot.
    ///
    /// The range must exactly match one enabled ordinary device range from the
    /// owned boot tree. Sensitive machine-control ranges and arbitrary physical
    /// memory are never eligible for this API.
    pub fn acquire(range: Range<usize>) -> Result<Self, IoMemError> {
        REGISTRY.lock().acquire(&range)?;
        Ok(Self {
            range,
            _not_sendable_by_layout_accident: PhantomData,
        })
    }

    /// Returns the size in bytes of the claimed range.
    pub fn len(&self) -> usize {
        self.range.end - self.range.start
    }

    /// Returns whether the claimed range is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Creates a borrowed view over a relative subrange.
    pub fn region(&self, range: Range<usize>) -> Result<IoMemRegion<'_>, IoMemError> {
        let absolute = checked_subrange(&self.range, range)?;
        Ok(IoMemRegion {
            range: absolute,
            owner: PhantomData,
        })
    }

    /// Performs one naturally aligned volatile scalar read.
    pub fn read_once<T: IoValue>(&self, offset: usize) -> Result<T, IoMemError> {
        read_once(&self.range, offset)
    }

    /// Performs one naturally aligned volatile scalar write.
    pub fn write_once<T: IoValue>(&self, offset: usize, value: T) -> Result<(), IoMemError> {
        write_once(&self.range, offset, value)
    }

    /// Validates one scalar register offset without accessing the device.
    pub fn validate<T: IoValue>(&self, offset: usize) -> Result<(), IoMemError> {
        checked_address::<T>(&self.range, offset).map(|_| ())
    }
}

/// A borrowed, bounded view into an [`IoMem`] capability.
pub struct IoMemRegion<'io> {
    range: Range<usize>,
    owner: PhantomData<&'io IoMem>,
}

/// Orders ordinary MMIO accesses against other RISC-V I/O and memory accesses.
pub fn io_fence() {
    // SAFETY: this closed ordering operation has no address or register input.
    unsafe { core::arch::asm!("fence iorw, iorw", options(nostack)) }
}

const ORDINARY_COMPATIBLES: [&str; 7] = [
    "ns16550a",
    "snps,dw-apb-uart",
    "xlnx,xps-uartlite-1.00.a",
    "bflb,bl808-uart",
    "sifive,uart0",
    "pl011",
    "sifive,test0",
];
const QEMU_MODEL: &str = "riscv-virtio,qemu";
const QEMU_UART_BASE: usize = 0x1000_0000;
const QEMU_TEST_BASE: usize = 0x0010_0000;

struct Registry {
    initialized: bool,
    sealed: bool,
    eligible: Vec<Range<usize>>,
    reserved: Vec<Range<usize>>,
    claimed: Vec<Range<usize>>,
}

static REGISTRY: Mutex<Registry> = Mutex::new(Registry {
    initialized: false,
    sealed: false,
    eligible: Vec::new(),
    reserved: Vec::new(),
    claimed: Vec::new(),
});

pub(crate) fn initialize(bytes: &[u8]) -> Result<(), IoMemError> {
    let fdt = Fdt::new(bytes).map_err(|_| IoMemError::InvalidRange)?;
    let mut eligible = Vec::new();
    let canonical_qemu = model(&fdt) == QEMU_MODEL;
    collect_ordinary_ranges(fdt.root(), canonical_qemu, &mut eligible)?;
    let mut registry = REGISTRY.lock();
    if registry.initialized {
        return Err(IoMemError::AlreadyClaimed);
    }
    if eligible
        .iter()
        .enumerate()
        .any(|(index, range)| eligible[..index].iter().any(|other| overlaps(range, other)))
    {
        return Err(IoMemError::InvalidRange);
    }
    registry.eligible = eligible;
    registry.initialized = true;
    Ok(())
}

fn collect_ordinary_ranges(
    node: FdtNode<'_>,
    canonical_qemu: bool,
    destination: &mut Vec<Range<usize>>,
) -> Result<(), IoMemError> {
    let compatible = node.property("compatible").and_then(|property| {
        property
            .as_str_list()
            .find(|compatible| ORDINARY_COMPATIBLES.contains(compatible))
    });
    if enabled(&node) && compatible.is_some() {
        let ranges = reg_ranges(node).map_err(|_| IoMemError::InvalidRange)?;
        for range in ranges {
            if TRUSTED_TARGET
                || canonical_qemu
                    && matches!(
                        (compatible, range.start),
                        (Some("ns16550a"), QEMU_UART_BASE) | (Some("sifive,test0"), QEMU_TEST_BASE)
                    )
            {
                destination.push(range);
            }
        }
    }
    for child in node.children() {
        collect_ordinary_ranges(child, canonical_qemu, destination)?;
    }
    Ok(())
}

pub(crate) fn reserve_ranges(ranges: &[Range<usize>]) -> Result<(), IoMemError> {
    let mut registry = REGISTRY.lock();
    if registry.sealed
        || ranges.iter().enumerate().any(|(index, range)| {
            invalid_range(range)
                || registry
                    .claimed
                    .iter()
                    .chain(registry.reserved.iter())
                    .any(|known| overlaps(range, known))
                || ranges[..index].iter().any(|known| overlaps(range, known))
        })
    {
        return Err(IoMemError::AlreadyClaimed);
    }
    registry.reserved.extend_from_slice(ranges);
    Ok(())
}

pub(crate) fn claimed_ranges() -> Vec<Range<usize>> {
    REGISTRY.lock().claimed.clone()
}

pub(crate) fn seal() {
    REGISTRY.lock().sealed = true;
}

impl Registry {
    fn acquire(&mut self, range: &Range<usize>) -> Result<(), IoMemError> {
        if !self.initialized || self.sealed || invalid_range(range) {
            return Err(IoMemError::InvalidRange);
        }
        if !self.eligible.contains(range) {
            return Err(IoMemError::OutOfBounds);
        }
        if self
            .reserved
            .iter()
            .chain(self.claimed.iter())
            .any(|known| overlaps(range, known))
        {
            return Err(IoMemError::AlreadyClaimed);
        }
        self.claimed.push(range.clone());
        Ok(())
    }
}

fn invalid_range(range: &Range<usize>) -> bool {
    range.start >= range.end || !range.start.is_multiple_of(4) || !range.end.is_multiple_of(4)
}

fn overlaps(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

impl IoMemRegion<'_> {
    /// Returns the view size in bytes.
    pub fn len(&self) -> usize {
        self.range.end - self.range.start
    }

    /// Returns whether the view is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Performs one naturally aligned volatile scalar read inside this view.
    pub fn read_once<T: IoValue>(&self, offset: usize) -> Result<T, IoMemError> {
        read_once(&self.range, offset)
    }

    /// Performs one naturally aligned volatile scalar write inside this view.
    pub fn write_once<T: IoValue>(&self, offset: usize, value: T) -> Result<(), IoMemError> {
        write_once(&self.range, offset, value)
    }
}

fn checked_subrange(
    owner: &Range<usize>,
    relative: Range<usize>,
) -> Result<Range<usize>, IoMemError> {
    if relative.start > relative.end {
        return Err(IoMemError::OutOfBounds);
    }
    let start = owner
        .start
        .checked_add(relative.start)
        .ok_or(IoMemError::OutOfBounds)?;
    let end = owner
        .start
        .checked_add(relative.end)
        .ok_or(IoMemError::OutOfBounds)?;
    (start <= end && end <= owner.end)
        .then_some(start..end)
        .ok_or(IoMemError::OutOfBounds)
}

fn checked_address<T: IoValue>(range: &Range<usize>, offset: usize) -> Result<usize, IoMemError> {
    let address = range
        .start
        .checked_add(offset)
        .ok_or(IoMemError::OutOfBounds)?;
    let end = address
        .checked_add(size_of::<T>())
        .ok_or(IoMemError::OutOfBounds)?;
    if end > range.end {
        return Err(IoMemError::OutOfBounds);
    }
    if !address.is_multiple_of(align_of::<T>()) {
        return Err(IoMemError::Misaligned);
    }
    Ok(address)
}

fn read_once<T: IoValue>(range: &Range<usize>, offset: usize) -> Result<T, IoMemError> {
    let address = checked_address::<T>(range, offset)?;
    // SAFETY: `checked_address` proved width, bounds and natural alignment.
    Ok(unsafe { T::read(address) })
}

fn write_once<T: IoValue>(range: &Range<usize>, offset: usize, value: T) -> Result<(), IoMemError> {
    let address = checked_address::<T>(range, offset)?;
    // SAFETY: `checked_address` proved width, bounds and natural alignment.
    unsafe { T::write(address, value) };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_are_relative_and_bounded() {
        let io = IoMem {
            range: 0x1000..0x1100,
            _not_sendable_by_layout_accident: PhantomData,
        };
        let region = io.region(0x20..0x40).unwrap();
        assert_eq!(region.range, 0x1020..0x1040);
        assert_eq!(region.len(), 0x20);
        assert_eq!(io.region(0x80..0x101).err(), Some(IoMemError::OutOfBounds));
    }

    #[test]
    fn scalar_validation_checks_width_and_alignment_without_accessing_mmio() {
        let range = 0x1000..0x1010;
        assert_eq!(checked_address::<u32>(&range, 4), Ok(0x1004));
        assert_eq!(
            checked_address::<u32>(&range, 2),
            Err(IoMemError::Misaligned)
        );
        assert_eq!(
            checked_address::<u32>(&range, 14),
            Err(IoMemError::OutOfBounds)
        );
    }
}
