//! Drivers for devices shared by multiple platforms.

pub(super) mod aia;
pub(super) mod cci;
pub(super) mod clint;
pub(super) mod console;
pub(super) mod plicsw;
pub(super) mod plmt;
pub(super) mod reset;

use super::{Description, match_desc};
