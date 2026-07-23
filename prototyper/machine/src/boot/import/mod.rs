//! Boot-input import for the supported boot protocols.

mod dynamic;
mod fixed;

pub(crate) use dynamic::*;
pub(crate) use fixed::*;
