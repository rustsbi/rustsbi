//! Boot-local platform discovery.

mod aia_facts;
mod clint_facts;
mod config;
mod console;
mod dt;
mod facts;
mod hart;
mod power;
mod timer_and_ipi;

pub(crate) use console::install as install_console;
pub(crate) use facts::discover;
pub(crate) use power::install as install_power;
pub(crate) use timer_and_ipi::install as install_timer_and_ipi;
