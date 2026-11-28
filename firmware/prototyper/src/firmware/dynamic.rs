//! The loader's [OpenSBI dynamic handoff](https://github.com/riscv-software-src/opensbi/blob/019a8e69a1dc0c0f011fabd0372e1ba80e40dd7c/include/sbi/fw_dynamic.h).

use riscv_csr_spec::standard::isa::values::status::Privilege;
use runtime::memory::{PhysAddr, PhysAddrRange};

/// A snapshot of the loader's record, independent of its lifetime after entry.
#[derive(Clone, Copy)]
pub(super) struct DynamicInfo {
    pub(super) storage: PhysAddrRange,
    pub(super) boot_hart: Option<usize>,
    next_addr: usize,
    next_mode: usize,
}

#[derive(Clone, Copy)]
pub(super) enum ReadError {
    Address(usize),
    Magic(usize),
    Version(usize),
}

impl ReadError {
    pub(super) fn fail(self) -> ! {
        match self {
            Self::Address(address) => error!("Invalid dynamic handoff address {address:#x}"),
            Self::Magic(magic) => error!("Invalid dynamic handoff magic {magic:#x}"),
            Self::Version(version) => error!("Unsupported dynamic handoff version {version}"),
        }
        crate::sbi::reset::fail()
    }
}

impl DynamicInfo {
    /// Reads the header, then only the words belonging to that version.
    ///
    /// # Safety
    ///
    /// For a nonzero, word-aligned address, the loader must supply a readable,
    /// initialized two-word header. A valid v1 or v2 header must be followed by
    /// the rest of that version's record. The bytes must remain stable during this call.
    pub(super) unsafe fn read(address: usize) -> Result<Self, ReadError> {
        let words = address as *const usize;
        let word_size = size_of::<usize>();
        if words.is_null() || !words.is_aligned() || address.checked_add(2 * word_size).is_none() {
            return Err(ReadError::Address(address));
        }
        // SAFETY: The caller supplies the two header words at this aligned address.
        let [magic, version] = unsafe { words.cast::<[usize; 2]>().read() };
        if magic != 0x4942_534f {
            return Err(ReadError::Magic(magic));
        }
        let word_count = match version {
            1 => 5,
            2 => 6,
            _ => return Err(ReadError::Version(version)),
        };
        let storage = PhysAddrRange::from_start_len(PhysAddr::new(address), word_count * word_size)
            .map_err(|_| ReadError::Address(address))?;
        // SAFETY: Both supported versions provide next_addr and next_mode after the header.
        let [next_addr, next_mode] = unsafe { words.add(2).cast::<[usize; 2]>().read() };
        let boot_hart = if version == 2 {
            // SAFETY: Only v2 defines a sixth word, following the options word.
            let hart = unsafe { words.add(5).read() };
            (hart != usize::MAX).then_some(hart)
        } else {
            None
        };
        Ok(Self {
            storage,
            boot_hart,
            next_addr,
            next_mode,
        })
    }

    pub(super) fn next_stage(self) -> (Privilege, usize) {
        if let Ok(mode) = Privilege::try_from(self.next_mode)
            && crate::cfg::DYNAMIC_NEXT_ADDR_RANGE
                .iter()
                .any(|range| (range.start as usize..range.end as usize).contains(&self.next_addr))
        {
            return (mode, self.next_addr);
        }
        error!(
            "Invalid dynamic handoff target: mode {:#x}, address {:#x}",
            self.next_mode, self.next_addr
        );
        crate::sbi::reset::fail()
    }
}
