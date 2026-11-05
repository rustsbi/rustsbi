//! Drivers for devices shared by multiple platforms.

pub(crate) mod cci;
pub(crate) mod clint;
pub(crate) mod console;
pub(crate) mod imsic;
pub(crate) mod plicsw;
pub(crate) mod plmt;
pub(crate) mod reset;
pub(crate) mod thead_plic;

use super::{Description, match_desc};
