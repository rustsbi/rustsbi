//! Boot-local platform discovery.

mod aia;
mod clint;
mod config;
mod console;
mod discovery;
mod dt;
mod hart;
mod power;
mod timer_and_ipi;

pub(crate) use console::install as install_console;
pub(crate) use discovery::{finish as finish_device_tree, memory, parse};
pub(crate) use hart::discover as discover_harts;
pub(crate) use power::install as install_power;
pub(crate) use timer_and_ipi::install as install_timer_and_ipi;
