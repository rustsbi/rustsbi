//! Machine information and counter-access registers.
use crate::{macros::csr, values::counter::CounterSet};

csr! {
    pub struct HartIdentifier(usize);

    /// Machine hart identifier (`mhartid`).
    HartId { 0xf14, r: HartIdentifier }
    /// Counter access permitted to lower privilege modes (`mcounteren`).
    CounterEnable { 0x306, r: CounterSet, w: CounterSet }
}
