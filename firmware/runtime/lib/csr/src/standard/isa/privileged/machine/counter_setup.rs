//! Machine counter setup.
use crate::{macros::csr, values::counter::CounterSet};

csr! {
    /// Machine counter-inhibit register (`mcountinhibit`).
    ///
    /// See
    /// <https://docs.riscv.org/reference/isa/priv/machine.html#mcountinhibit>.
    CounterInhibit { 0x320, r: CounterSet }
}
