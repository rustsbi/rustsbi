//! CLINT timer drivers, one per register layout.

mod sifive;
mod thead;

pub(super) use sifive::SIFIVE;
pub(super) use thead::THEAD;
