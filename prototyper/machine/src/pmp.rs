//! Semantic physical-memory protection and exact RISC-V PMP installation.
//!
//! Machine-owned ranges form an immutable high-priority deny floor. Upper
//! firmware policy explicitly grants S/U access to named ranges; unmatched
//! addresses remain inaccessible.
//!
//! TODO: Gate DMA-capable device publication and next-stage visibility on
//! IOPMP/IOMPT, a suitably controlled IOMMU, or equivalent bus-level isolation.
//! Until then malicious device-initiated writes are outside the soundness claim.

mod entry;
mod hardware;
mod hardware_core;
mod policy;
mod state;

pub(crate) use entry::{configure_current_hart, machine_image_range, publish};
#[cfg(test)]
use policy::compile;
#[cfg(test)]
use state::*;
pub use state::{Configuration, Permissions, PmpError};

/// Builds one immutable lower-privilege PMP configuration.
///
/// Each left-hand expression must evaluate to `core::ops::Range<usize>`.
/// Machine-owned ranges are denied even if they overlap a listed grant.
///
/// ```
/// # use rustsbi_prototyper_machine::pmp;
/// let ram = 0x8000_0000..0x8800_0000;
/// let uart = 0x1000_0000..0x1000_1000;
/// let configuration = pmp::config! {
///     ram => [read, write, execute];
///     uart => [read, write];
/// }.unwrap();
/// # let _ = configuration;
/// ```
#[macro_export]
#[doc(hidden)]
macro_rules! __pmp_config {
    ($($range:expr => [$($permission:ident),+ $(,)?]);+ $(;)?) => {{
        (|| -> ::core::result::Result<
            $crate::pmp::Configuration,
            $crate::pmp::PmpError,
        > {
            let mut configuration = $crate::pmp::Configuration::empty();
            $(
                let mut permissions = $crate::pmp::Permissions::empty();
                $(
                    permissions |= $crate::__pmp_permission!($permission);
                )+
                configuration.grant($range, permissions)?;
            )+
            ::core::result::Result::Ok(configuration)
        })()
    }};
}

#[macro_export]
#[doc(hidden)]
macro_rules! __pmp_permission {
    (read) => {
        $crate::pmp::Permissions::READ
    };
    (write) => {
        $crate::pmp::Permissions::WRITE
    };
    (execute) => {
        $crate::pmp::Permissions::EXECUTE
    };
}

pub use crate::__pmp_config as config;

#[cfg(test)]
mod tests;
