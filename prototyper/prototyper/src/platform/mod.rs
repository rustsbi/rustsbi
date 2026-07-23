//! Boot-local platform discovery.

mod aia_facts;
mod clint_facts;
mod config;
mod console_facts;
mod dt;
mod facts;
mod hart;
mod power;
mod timer_and_ipi;

pub(crate) use facts::discover;
pub(crate) use timer_and_ipi::install as install_timer_and_ipi;
