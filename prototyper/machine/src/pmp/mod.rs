//! Semantic physical-memory protection policy and exact static encoding.
//!
//! The compatibility-oriented policy gives machine-owned ranges higher-priority
//! deny entries and places one broad lower-priority S/U allow entry last. That
//! layout is not a complete soundness proof by itself: every machine-sensitive
//! memory/MMIO interval must be classified, and PMP constrains hart-originated
//! accesses only.
//!
//! TODO: Gate DMA-capable device publication and next-stage visibility on
//! IOPMP/IOMPT, a suitably controlled IOMMU, or equivalent bus-level isolation.
//! Until then malicious device-initiated writes are outside the soundness claim.

mod entry;
mod hardware;
mod policy;
mod state;

pub(crate) use entry::{configure_current_hart, machine_image_range, publish};
#[cfg(test)]
use policy::{compile, compile_machine_policy};
#[cfg(test)]
use state::*;

#[cfg(test)]
mod tests;
