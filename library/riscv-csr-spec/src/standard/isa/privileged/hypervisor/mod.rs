//! Hypervisor-level CSR definitions.
//!
//! The [hypervisor state-enable registers](https://docs.riscv.org/reference/isa/v20260120/priv/smstateen.html#3-1-1-state-enable-extensions)
//! restrict guest access to state enabled by `mstateen0..3`.
//! Each `SE0` field controls the corresponding `sstateenN` access.
//!
//! [`hgeie` and `hgeip`](https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#hgeinterruptregs)
//! identify guest external interrupt sources by number. The
//! [interrupt pending, enable and delegation registers](https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#hinterruptregs)
//! index their bits by interrupt cause.
//!
//! Coordinate updates to `Htval` and `Htinst` with HS-mode trap handling.

mod address_translation;
mod configuration;
mod counter_setup;
mod counter_timer;
mod guest_interrupt;
mod state_enable;
mod trap_handling;
mod trap_setup;

pub use address_translation::*;
pub use configuration::*;
pub use counter_setup::*;
pub use counter_timer::*;
pub use guest_interrupt::*;
pub use state_enable::*;
pub use trap_handling::*;
pub use trap_setup::*;
