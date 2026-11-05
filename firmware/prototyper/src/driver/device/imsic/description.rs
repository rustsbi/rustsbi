//! Parsed machine IMSIC resources.

use alloc::{boxed::Box, vec::Vec};
use runtime::{
    Result,
    ipi::{External, Iid, IpiDevice},
    memory::{MemoryRegistry, PhysAddr, RegRange},
};

use super::super::Description;
use super::device::ImsicIpi;

/// FDT `compatible` strings identifying an IMSIC interrupt controller.
pub(crate) const IMSIC_COMPATIBLES: [&str; 1] = ["riscv,imsics"];

/// Size of one IMSIC interrupt file.
pub(crate) const IMSIC_FILE_SPAN: usize = 1usize << 12;

/// Address layout of the machine-level IMSIC interrupt files.
pub(crate) struct AddressLayout {
    /// Base address of the machine-level interrupt files.
    pub(crate) machine_base: PhysAddr,
    /// Width of the hart-index field.
    pub(crate) hart_index_bits: u32,
    /// Bit position of the group-index field.
    group_index_shift: u32,
    /// Bit position of the hart-index field.
    hart_index_shift: u32,
}

impl AddressLayout {
    /// Creates an address layout from validated index widths.
    pub(crate) const fn new(
        machine_base: PhysAddr,
        hart_index_bits: u32,
        group_index_shift: u32,
        hart_index_shift: u32,
    ) -> Self {
        Self {
            machine_base,
            hart_index_bits,
            group_index_shift,
            hart_index_shift,
        }
    }

    /// Returns the machine interrupt-file address for a hart and group index.
    ///
    /// Returns `None` if the address arithmetic overflows.
    pub(crate) fn machine_file_address(
        &self,
        hart_index: u32,
        group_index: u32,
    ) -> Option<PhysAddr> {
        let group_offset = if group_index == 0 {
            0
        } else {
            usize::try_from(group_index)
                .ok()?
                .checked_shl(self.group_index_shift)?
        };
        let hart_offset = usize::try_from(hart_index)
            .ok()?
            .checked_shl(self.hart_index_shift)?;
        self.machine_base
            .checked_add(group_offset)?
            .checked_add(hart_offset)
    }
}

/// Parsed machine IMSIC resources before acquisition.
pub(crate) struct Desc {
    pub(crate) layout: AddressLayout,
    pub(crate) ipi_iid: Iid,
    pub(crate) num_iids: u16,
    pub(crate) hart_files: Vec<RegRange>,
}

impl Description for Desc {
    type Device = Box<dyn IpiDevice<External>>;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        Ok(Box::new(ImsicIpi::init(*self, memory)?))
    }
}
