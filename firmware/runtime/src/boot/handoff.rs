//! Owned snapshots of the previous stage's `fw_dynamic` handoff.
//!
//! The ABI uses packed, native-endian XLEN words.
//! Version 2 adds `boot_hart` after the five-word version 1 layout:
//! <https://github.com/riscv-software-src/opensbi/blob/019a8e69a1dc0c0f011fabd0372e1ba80e40dd7c/include/sbi/fw_dynamic.h>.

use core::{fmt, mem::size_of};

use crate::{memory::PhysAddr, trap};

const MAGIC: usize = 0x4942534f;
const WORD: usize = size_of::<usize>();
const LEGACY_STORAGE_WORDS: usize = 5;
const VERSION_TWO_STORAGE_WORDS: usize = 6;

pub(super) const MAX_STORAGE_BYTES: usize = VERSION_TWO_STORAGE_WORDS * WORD;

pub(super) const fn storage_size_bytes(version: usize) -> Option<usize> {
    match version {
        0 | 1 => Some(LEGACY_STORAGE_WORDS * WORD),
        2 => Some(MAX_STORAGE_BYTES),
        _ => None,
    }
}

/// Copies the entry register's packed handoff into owned values.
///
/// Guarded byte loads support unaligned storage and early access faults.
/// Versions 0 and 1 use the five-word layout and elect any enabled hart.
///
/// # Safety
///
/// A nonzero address must designate immutable foreign storage, never MMIO or
/// a live Rust allocation, reserved until every receiving hart has copied it.
/// All receiving harts must observe the same bytes. M-mode entry must have
/// cleared MPRV/MIE and installed Runtime's early vector and stack.
pub(super) unsafe fn snapshot(address: usize) -> Result<DynamicInfo, DynamicReadError> {
    let address = PhysAddr::new(address);
    let check_span = |size_bytes| {
        if address.as_usize() == 0 || address.checked_add(size_bytes - 1).is_none() {
            return Err(DynamicReadError::InvalidAddress { address });
        }
        Ok(())
    };
    check_span(2 * WORD)?;
    let read_word = |index| {
        let mut encoded = [0; WORD];
        for (offset, byte) in encoded.iter_mut().enumerate() {
            let byte_address = address.as_usize() + index * WORD + offset;
            // SAFETY:
            // 1. Entry grants immutable foreign storage and an M-mode recovery stack/vector.
            // 2. snapshot checks the complete span before reading these packed words.
            *byte = unsafe { trap::read_boot_byte_guarded(byte_address) }
                .map_err(|source| DynamicReadError::Access { address, source })?;
        }
        Ok(usize::from_ne_bytes(encoded))
    };
    let magic = read_word(0)?;
    let version = read_word(1)?;
    let storage_size = storage_size_bytes(version);
    let Some(size_bytes) = storage_size.filter(|_| magic == MAGIC) else {
        return Err(DynamicReadError::InvalidHeader {
            invalid_magic: (magic != MAGIC).then_some(magic),
            unsupported_version: storage_size.is_none().then_some(version),
        });
    };
    check_span(size_bytes)?;
    Ok(DynamicInfo {
        magic,
        version,
        next_addr: read_word(2)?,
        next_mode: read_word(3)?,
        options: read_word(4)?,
        boot_hart: if version >= 2 {
            read_word(5)?
        } else {
            usize::MAX
        },
    })
}

/// An owned `fw_dynamic` value, with absent legacy fields normalized.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DynamicInfo {
    /// Dynamic information magic value.
    pub magic: usize,
    /// Version of dynamic information.
    pub version: usize,
    /// Address of the next boot-loading stage.
    pub next_addr: usize,
    /// RISC-V privilege mode of the next boot-loading stage.
    pub next_mode: usize,
    /// Firmware options defined by the selected SBI implementation.
    pub options: usize,
    /// Preferred boot hart, or `usize::MAX` to elect one by race.
    pub boot_hart: usize,
}

/// Failure to copy or validate the previous stage's dynamic handoff.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicReadError {
    /// The handoff address is null or its required span wraps.
    InvalidAddress {
        /// Address supplied at firmware entry.
        address: PhysAddr,
    },
    /// The header does not describe a supported `fw_dynamic` layout.
    InvalidHeader {
        /// Unexpected magic, when present.
        invalid_magic: Option<usize>,
        /// Unsupported ABI version, when present.
        unsupported_version: Option<usize>,
    },
    /// A guarded physical load failed while copying the handoff.
    Access {
        /// Start of the handoff buffer.
        address: PhysAddr,
        /// Original trap error, including its precise cause and fault address.
        source: trap::Error,
    },
}

impl fmt::Display for DynamicReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAddress { address } => write!(
                formatter,
                "invalid dynamic handoff address {:#x}",
                address.as_usize()
            ),
            Self::InvalidHeader {
                invalid_magic,
                unsupported_version,
            } => write!(
                formatter,
                "invalid dynamic handoff header: magic {invalid_magic:?}, version {unsupported_version:?}"
            ),
            Self::Access { address, source } => write!(
                formatter,
                "dynamic handoff at {:#x}: {source:?}",
                address.as_usize()
            ),
        }
    }
}
