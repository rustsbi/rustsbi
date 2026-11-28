//! Supervisor-level CSR definitions.
//!
//! The [supervisor state-enable registers](https://docs.riscv.org/reference/isa/v20260120/priv/smstateen.html#3-1-1-state-enable-extensions)
//! control U-mode access to state enabled by higher privilege levels.
//! [`satp`](https://docs.riscv.org/reference/isa/v20260120/priv/supervisor.html#satp)
//! holds the translation mode, address-space ID and root page-table PPN.
//!
//! Coordinate updates to `Scause` and `Stval` with active S-mode trap handling.

// Privileged ISA §1.1.2.2, Table 3 distinguishes trap setup from trap handling.
// Group counter controls and extension families such as Sstc by function so their
// related registers stay together across the table's categories.
// https://docs.riscv.org/reference/isa/v20260120/priv/priv-csrs.html#scsrnames
mod address_translation;
mod configuration;
mod counter_setup;
mod indirect;
mod state_enable;
mod timer_compare;
mod trap_handling;
mod trap_setup;

pub use address_translation::*;
pub use configuration::*;
pub use counter_setup::*;
pub use indirect::*;
pub use state_enable::*;
pub use timer_compare::*;
pub use trap_handling::*;
pub use trap_setup::*;
