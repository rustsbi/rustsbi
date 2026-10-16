//! Machine counter setup.
use crate::{macros::csr, values::counter::CounterSet};

csr! {
    /// Counter increment inhibition (`mcountinhibit`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/machine.html#mcountinhibit>
    CounterInhibit { 0x320, r: CounterSet }
}
