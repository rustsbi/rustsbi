//! Guest physical address translation and protection.

use crate::macros::csr;
use crate::values::xlen::Xlen;

csr! {
    /// Guest address-space configuration, including the virtual machine ID.
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/hypervisor.html#hgatp>.
    pub struct GuestAddressSpace(usize);

    /// Hypervisor guest address translation and protection register (`hgatp`).
    /// See <https://docs.riscv.org/reference/isa/priv/hypervisor.html#hgatp>.
    GuestAddressTranslation { 0x680, r: GuestAddressSpace,
        /// Replaces the guest address-space configuration.
        ///
        /// # Safety
        /// Preserve mappings used by live guest accesses and coordinate guest
        /// execution, `MPRV` accesses, and translation-cache invalidations.
        unsafe w: GuestAddressSpace
    }
}

impl GuestAddressSpace {
    #[cfg(target_pointer_width = "32")]
    const VMID_SHIFT: u32 = 22;
    #[cfg(target_pointer_width = "64")]
    const VMID_SHIFT: u32 = 44;
    #[cfg(target_pointer_width = "32")]
    const VMID_MASK: usize = 0x7f;
    #[cfg(target_pointer_width = "64")]
    const VMID_MASK: usize = 0x3fff;

    /// Returns the virtual machine ID from this snapshot.
    #[inline]
    pub const fn vmid(self) -> usize {
        (self.0 >> Self::VMID_SHIFT) & Self::VMID_MASK
    }

    /// Returns the virtual machine ID from this snapshot using `xlen` (`HSXLEN`).
    ///
    /// Returns `None` if `xlen` is [`Xlen::X128`] or exceeds this target's pointer width.
    #[inline]
    pub const fn vmid_for(self, xlen: Xlen) -> Option<usize> {
        match xlen {
            Xlen::X32 => Some((self.0 >> 22) & 0x7f),
            #[cfg(target_pointer_width = "64")]
            Xlen::X64 => Some((self.0 >> 44) & 0x3fff),
            #[cfg(target_pointer_width = "32")]
            Xlen::X64 => None,
            Xlen::X128 => None,
        }
    }

    /// Encodes `MODE = Bare` (`0`) and `PPN = 0` with `VMID = vmid` for `xlen` (`HSXLEN`).
    ///
    /// Returns `None` if `vmid` exceeds the field width for `xlen`, `xlen` is
    /// [`Xlen::X128`], or `xlen` exceeds this target's pointer width.
    #[inline]
    pub const fn for_vmid(vmid: usize, xlen: Xlen) -> Option<Self> {
        match xlen {
            Xlen::X32 => {
                if vmid <= 0x7f {
                    Some(Self(vmid << 22))
                } else {
                    None
                }
            }
            #[cfg(target_pointer_width = "64")]
            Xlen::X64 => {
                if vmid <= 0x3fff {
                    Some(Self(vmid << 44))
                } else {
                    None
                }
            }
            #[cfg(target_pointer_width = "32")]
            Xlen::X64 => None,
            Xlen::X128 => None,
        }
    }

    /// Returns a copy with the virtual machine ID replaced.
    ///
    /// The translation root and mode are preserved. Bits outside the
    /// architectural virtual machine ID field are discarded from `vmid`.
    /// When `MODE` is Bare, the specification requires the remaining fields
    /// to be zero; callers must meet that requirement before writing the value.
    #[inline]
    pub const fn with_vmid(self, vmid: usize) -> Self {
        Self(
            (self.0 & !(Self::VMID_MASK << Self::VMID_SHIFT))
                | ((vmid & Self::VMID_MASK) << Self::VMID_SHIFT),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vmid_for_and_for_vmid_respect_hsxlen_widths() {
        let encoded_32 = GuestAddressSpace::for_vmid(0x7f, Xlen::X32).unwrap();
        assert_eq!(usize::from(encoded_32), 0x7f << 22);
        assert_eq!(encoded_32.vmid_for(Xlen::X32), Some(0x7f));
        assert_eq!(GuestAddressSpace::for_vmid(0x80, Xlen::X32), None);
        assert_eq!(GuestAddressSpace::for_vmid(0, Xlen::X128), None);
        assert_eq!(encoded_32.vmid_for(Xlen::X128), None);

        #[cfg(target_pointer_width = "64")]
        {
            let encoded_64 = GuestAddressSpace::for_vmid(0x3fff, Xlen::X64).unwrap();
            assert_eq!(usize::from(encoded_64), 0x3fff << 44);
            assert_eq!(encoded_64.vmid_for(Xlen::X64), Some(0x3fff));
            assert_eq!(GuestAddressSpace::for_vmid(0x4000, Xlen::X64), None);
        }
        #[cfg(target_pointer_width = "32")]
        {
            assert_eq!(GuestAddressSpace::for_vmid(0, Xlen::X64), None);
            assert_eq!(encoded_32.vmid_for(Xlen::X64), None);
        }
    }
}
