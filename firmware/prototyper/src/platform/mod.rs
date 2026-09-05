#![forbid(unsafe_code)]

//! Platform discovery, initialization, and shared services.
//!
//! The boot hart turns the Runtime-owned Platform Description into [`BoardInfo`], binds
//! drivers to registered resources, and then publishes the resulting services
//! for all harts. Parsing and synchronization remain private to this module.

mod boot;
mod discovery;
mod error;
mod info;
mod report;
mod state;

pub(crate) mod qemu_aplic;

pub use boot::{firmware_ram_range, init_board, initialize_secondary_hart, wait_until_ready};
pub(crate) use info::{BoardInfo, ImsicInfo};
pub(crate) use state::{
    board_info, console_device, enabled_harts, mark_hart_privilege_checked,
    retain_privilege_checked_harts,
};
