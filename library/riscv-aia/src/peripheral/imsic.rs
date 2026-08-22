//! Incoming MSI Controller (IMSIC) peripheral.

use volatile_register::WO;

/// Incoming MSI Controller (IMSIC) register block.
///
/// Each interrupt file in an IMSIC has one or two memory-mapped 32-bit
/// registers for receiving MSI writes. These memory-mapped registers are
/// located within a naturally aligned 4-KiB region (a page) of physical
/// address space that exists for the interrupt file.
///
/// The rest of the 4-KiB page is reserved and read-only zeros.
#[repr(C, align(4096))]
pub struct Imsic {
    /// 0x000 - Set interrupt-pending bit by number, little-endian.
    pub seteipnum_le: WO<u32>,
    /// 0x004 - Set interrupt-pending bit by number, big-endian.
    pub seteipnum_be: WO<u32>,
    /// 0x008..0xFFF
    _reserved: [u32; 0x3fe],
}

/// MSI (Message-Signaled Interrupt) encoding utilities.
pub mod msi {
    /// Encodes an interrupt identity into little-endian MSI data.
    #[inline]
    pub const fn encode_le(identity: u16) -> u32 {
        identity as u32
    }
}

/// System-level IMSIC address calculation.
pub mod system {
    /// IMSIC address layout parameters.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct AddressLayout {
        /// Base address for machine-level interrupt files.
        pub machine_base: usize,
        /// Number of bits for the hart index.
        pub hart_index_bits: u32,
        /// Bit position of the group index.
        pub group_bits: u32,
        /// Bit position of the hart index.
        pub hart_offset_bits: u32,
    }

    impl AddressLayout {
        /// Calculates the address of a machine-level interrupt file.
        pub const fn machine_interrupt_file_address(&self, hart_id: u32, group_id: u32) -> usize {
            self.machine_base
                + (group_id << self.group_bits) as usize
                + (hart_id << self.hart_offset_bits) as usize
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Imsic;
    use core::mem::{align_of, size_of};
    use memoffset::offset_of;

    #[test]
    fn imsic_interrupt_file_layout() {
        assert_eq!(offset_of!(Imsic, seteipnum_le), 0x000);
        assert_eq!(offset_of!(Imsic, seteipnum_be), 0x004);
        assert_eq!(size_of::<Imsic>(), 0x1000);
        assert_eq!(align_of::<Imsic>(), 0x1000);
    }
}
