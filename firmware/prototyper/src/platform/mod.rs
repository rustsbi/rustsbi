#![forbid(unsafe_code)]

//! Platform discovery, initialization, and shared services.
//!
//! The boot hart turns the Runtime-owned Platform Description into [`info::BoardInfo`], binds
//! drivers to registered resources, and then publishes the resulting services
//! for all harts. Discovery facts are released after startup reporting.
//!
//! SoC startup adapters such as SpacemiT K1 coordinate hart preparation and
//! coherency. QEMU APLIC setup describes the interrupt-controller wiring of
//! the QEMU `virt` machine.

pub(crate) mod allwinner;
mod boot;
mod devices;
mod discovery;
mod error;
mod handoff;
mod info;
mod interrupts;
pub(crate) mod protection;
mod report;
mod state;

pub(crate) mod qemu_aplic;

pub use boot::{firmware_ram_range, init_board, initialize_secondary_hart};
pub(crate) use info::ImsicInfo;
pub(crate) use state::{
    console_device, enabled_harts, hart_privilege_checked, interrupts, mark_hart_privilege_checked,
    time_source, wait_until_ready,
};
