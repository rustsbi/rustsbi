//! Device-class drivers and SoC-specific register protocols.
//!
//! Platform selects devices and owns them after publication. Runtime borrows
//! the hardware operations and manages architectural CSR and interrupt state.

pub(crate) mod aia;
pub(crate) mod allwinner;
pub(crate) mod aplic;
mod cci;
pub(crate) mod clint;
mod console;
pub(crate) mod plicsw;
pub(crate) mod plmt;
mod reset;
pub(crate) mod spacemit;
pub(crate) mod thead;

pub(crate) use reset::{
    Description as ResetDescription, ResetController, ResetError, ResetReason, ResetRequest,
    ResetType,
};

pub(crate) use aia::{IMSIC_COMPATIBLES, IMSIC_FILE_SPAN};
pub(crate) use cci::Cci550;
pub(crate) use clint::ClintKind;
pub(crate) use console::{Console, ConsoleError, ConsoleKind};

pub(crate) const PLMT_COMPATIBLE: &str = "andestech,plmt0";
pub(crate) const SUNXI_PLICSW_COMPATIBLE: &str = "allwinner,sun300i-plicsw";

pub(crate) const THEAD_PLIC_COMPATIBLES: [&str; 2] =
    ["thead,c900-plic", "allwinner,thead,c900-plic"];
