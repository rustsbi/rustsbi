use core::marker::PhantomData;

/// Physical slice wrapper with type annotation.
///
/// This struct wraps slices in RISC-V physical memory by low and high part of the
/// physical base address as well as its length. It is usually used by SBI extensions
/// as parameter types to pass base address and length parameters on physical memory
/// other than a virtual one.
///
/// Generic parameter `P` represents a hint of how this physical slice would be used.
/// For example, `Physical<&[u8]>` represents an immutable reference to physical byte slice,
/// while `Physical<&mut [u8]>` represents a mutable one.
///
/// An SBI implementation should load or store memory using both `phys_addr_lo` and
/// `phys_addr_hi` combined as base address. A supervisor program (kernels etc.)
/// should provide continuous physical memory, wrapping its reference using this structure
/// before passing into SBI runtime.
#[derive(Clone, Copy)]
pub struct Physical<P> {
    num_bytes: usize,
    phys_addr_lo: usize,
    phys_addr_hi: usize,
    _marker: PhantomData<P>,
}

impl<P> Physical<P> {
    /// Create a physical memory slice by length and physical address.
    #[inline]
    pub const fn new(num_bytes: usize, phys_addr_lo: usize, phys_addr_hi: usize) -> Self {
        Self {
            num_bytes,
            phys_addr_lo,
            phys_addr_hi,
            _marker: core::marker::PhantomData,
        }
    }

    /// Returns length of the physical memory slice.
    #[inline]
    pub const fn num_bytes(&self) -> usize {
        self.num_bytes
    }

    /// Returns low-part base address of physical memory slice.
    #[inline]
    pub const fn phys_addr_lo(&self) -> usize {
        self.phys_addr_lo
    }

    /// Returns high-part base address of physical memory slice.
    #[inline]
    pub const fn phys_addr_hi(&self) -> usize {
        self.phys_addr_hi
    }
}
