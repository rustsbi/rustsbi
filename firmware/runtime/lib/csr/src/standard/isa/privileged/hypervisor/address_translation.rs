//! Guest physical address translation and protection.

use crate::macros::csr;

csr! {
    /// Guest address-space configuration, including the virtual machine ID.
    ///
    /// Field helpers use the target pointer width. `hgatp` itself uses HSXLEN;
    /// Runtime must check that HSXLEN matches before decoding or replacing VMID.
    /// Raw bits alone do not establish that width or permit a guest context switch.
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
    /// Decodes a native-width `hgatp` snapshot.
    ///
    /// The caller must establish that the runtime HSXLEN matches the target
    /// pointer width before interpreting or writing the fields.
    #[inline]
    pub const fn from_bits(bits: usize) -> Self {
        Self(bits)
    }

    /// Returns the encoded native-width `hgatp` bits.
    #[inline]
    pub const fn bits(self) -> usize {
        self.0
    }

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
