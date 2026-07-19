//! Boot-local platform discovery.

mod aia_facts;
mod clint_facts;
mod config;
mod console_facts;
mod dt;
mod facts;
mod hart;
mod power;

pub(crate) use facts::discover;
