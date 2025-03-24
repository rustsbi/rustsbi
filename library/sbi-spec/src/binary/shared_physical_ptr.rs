use core::marker::PhantomData;

/// Shared memory physical address raw pointer with type annotation.
///
/// This is a structure wrapping a raw pointer to the value of the type `T` without
/// a pointer metadata. `SharedPtr`'s are _thin_; they won't include metadata
/// as RISC-V SBI does not provide an approach to pass them via SBI calls,
/// thus the length of type `T` should be decided independently of raw
/// pointer structure.
///
/// `SharedPtr` can be used as a parameter to pass the shared memory physical pointer
///  with a given base address in RISC-V SBI calls. For example, a `SharedPtr<[u8; 64]>`
/// would represent a fixed-size 64 byte array on a RISC-V SBI function argument
/// type.
///
/// This structure cannot be dereferenced directly with physical addresses,
/// because on RISC-V systems the physical address space could be larger than the
/// virtual ones. Hence, this structure describes the physical memory range by
/// two `usize` values: the upper `phys_addr_hi` and lower `phys_addr_lo`.
///
/// RISC-V SBI extensions may declare special pointer values for shared memory
/// raw pointers. For example, SBI STA declares that steal-time information
/// should stop from reporting when the SBI call is invoked using all-ones
/// bitwise shared pointer, i.e. `phys_addr_hi` and `phys_addr_lo` both equals
/// `usize::MAX`. `SharedPtr` can be constructed using such special values
/// by providing them to the `SharedPtr::new` function.
///
/// # Requirements
///
/// If an SBI function needs to pass a shared memory physical address range to
/// the SBI implementation (or higher privilege mode), then this physical memory
/// address range MUST satisfy the following requirements:
///
/// * The SBI implementation MUST check that the supervisor-mode software is
///   allowed to access the specified physical memory range with the access
///   type requested (read and/or write).
/// * The SBI implementation MUST access the specified physical memory range
///   using the PMA attributes.
/// * The data in the shared memory MUST follow little-endian byte ordering.
///
/// *NOTE:* If the supervisor-mode software accesses the same physical memory
/// range using a memory type different from the PMA, then a loss of coherence
/// or unexpected memory ordering may occur. The invoking software should
/// follow the rules and sequences defined in the RISC-V Svpbmt specification
/// to prevent the loss of coherence and memory ordering.
///
/// It is recommended that a memory physical address passed to an SBI function
/// should use at least two `usize` parameters to support platforms
/// which have memory physical addresses wider than `XLEN` bits.
// FIXME: should constrain with `T: Thin` once ptr_metadata feature is stabled;
// RISC-V SBI does not provide an approach to pass pointer metadata by SBI calls.
pub struct SharedPtr<T> {
    phys_addr_lo: usize,
    phys_addr_hi: usize,
    _marker: PhantomData<*mut T>,
}

// FIXME: we should consider strict provenance rules for this pointer-like structure
// once feature strict_provenance is stabled.
impl<T> SharedPtr<T> {
    /// Create a shared physical memory pointer by physical address.
    #[inline]
    pub const fn new(phys_addr_lo: usize, phys_addr_hi: usize) -> Self {
        Self {
            phys_addr_lo,
            phys_addr_hi,
            _marker: PhantomData,
        }
    }

    /// Returns low-part physical address of the shared physical memory pointer.
    #[inline]
    pub const fn phys_addr_lo(self) -> usize {
        self.phys_addr_lo
    }

    /// Returns high-part physical address of the shared physical memory pointer.
    #[inline]
    pub const fn phys_addr_hi(self) -> usize {
        self.phys_addr_hi
    }
}

impl<T> Clone for SharedPtr<T> {
    #[inline(always)]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for SharedPtr<T> {}
