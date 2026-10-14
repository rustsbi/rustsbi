//! Machine information registers.
use crate::macros::csr;

csr! {
    pub struct HartIdentifier(usize);

    /// Machine hart identifier (`mhartid`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/machine.html#mhartid>
    HartId { 0xf14, r: HartIdentifier }
}
