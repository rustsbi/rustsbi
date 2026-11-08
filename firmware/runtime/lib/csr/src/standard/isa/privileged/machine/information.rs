//! Machine information registers.
use crate::macros::csr;

csr! {
    pub struct HartIdentifier(usize);
    /// A raw machine identification register value.
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#_machine_information_registers>.
    pub struct Identifier(usize);

    /// Vendor identifier (`mvendorid`).
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mvendorid>.
    VendorId { 0xf11, r: Identifier }
    /// Architecture identifier (`marchid`).
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#marchid>.
    ArchitectureId { 0xf12, r: Identifier }
    /// Implementation identifier (`mimpid`).
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mimpid>.
    ImplementationId { 0xf13, r: Identifier }

    /// Hardware hart identifier (`mhartid`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mhartid>.
    HartId { 0xf14, r: HartIdentifier }
}

impl Identifier {
    /// Constructs a machine identification value from its raw bits.
    pub const fn from_bits(bits: usize) -> Self {
        Self(bits)
    }
}
