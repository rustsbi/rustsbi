//! Shared memory physical address range parameter

use core::marker::PhantomData;

/// Shared memory physical address range with type annotation
///
/// This is a wrapper around a kind of pointer to represent that its value is
/// indexed on physical address space.
///
/// This structure cannot be dereferenced directly with physical addresses,
/// because on RISC-V systems the physical address space could be larger than the
/// virtual ones. Hence, this structure describes physical memory range by
/// two `usize` values: the upper `phys_addr_hi` and lower `phys_addr_lo`.
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
/// range using a memory type different than the PMA, then a loss of coherence
/// or unexpected memory ordering may occur. The invoking software should
/// follow the rules and sequences defined in the RISC-V Svpbmt specification
/// to prevent the loss of coherence and memory ordering.
///
/// It is recommended that a memory physical address passed to an SBI function
/// should use at least two `usize` parameters to support platforms
/// which have memory physical addresses wider than `XLEN` bits.
#[derive(Clone, Copy)]
pub struct Physical<P> {
    num_bytes: usize,
    phys_addr_lo: usize,
    phys_addr_hi: usize,
    _marker: PhantomData<P>,
}

impl<P> Physical<P> {
    /// Create a physical memory range by length and physical address.
    #[inline]
    pub const fn new(num_bytes: usize, phys_addr_lo: usize, phys_addr_hi: usize) -> Self {
        Self {
            num_bytes,
            phys_addr_lo,
            phys_addr_hi,
            _marker: core::marker::PhantomData,
        }
    }

    /// Returns length of the physical address range.
    #[inline]
    pub const fn num_bytes(&self) -> usize {
        self.num_bytes
    }

    /// Returns low part of physical address range.
    #[inline]
    pub const fn phys_addr_lo(&self) -> usize {
        self.phys_addr_lo
    }

    /// Returns high part of physical address range.
    #[inline]
    pub const fn phys_addr_hi(&self) -> usize {
        self.phys_addr_hi
    }
}
