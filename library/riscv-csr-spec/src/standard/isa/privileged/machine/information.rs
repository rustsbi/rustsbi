//! Machine identification and configuration pointers.

use crate::standard::isa::values::address::Address;

csr_value! {
    /// Machine identification register word (`mvendorid`, `marchid`, `mimpid`).
    pub struct Identifier;

    /// Hardware thread identifier (`mhartid`).
    #[derive_ord(Ord, PartialOrd)]
    pub struct HartIdentifier;
}

define_csr! {
    /// The [JEDEC manufacturer ID](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#2-1-1-2-machine-vendor-id-mvendorid-register) of the core provider, or zero if unreported.
    Mvendorid { 0xf11, r: Identifier };

    /// Machine architecture identifier (`marchid`).
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#2-1-1-3-machine-architecture-id-marchid-register>.
    Marchid { 0xf12, r: Identifier };

    /// Machine implementation identifier (`mimpid`).
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#2-1-1-4-machine-implementation-id-mimpid-register>.
    Mimpid { 0xf13, r: Identifier };

    /// The [hardware thread ID](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#2-1-1-5-hart-id-mhartid-register), unique within this execution environment.
    Mhartid { 0xf14, r: HartIdentifier };

    /// The physical address of the configuration data structure, or zero if absent.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#2-1-1-17-machine-configuration-pointer-mconfigptr-register>.
    Mconfigptr { 0xf15, r: Address };
}
