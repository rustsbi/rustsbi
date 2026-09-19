//! Allwinner firmware drivers.
//!
//! [`v821`] owns devices used only by custom SBI extensions. [`v861`] adapts
//! V861's C907 power controls to the generic HSM hart-wakeup interface.

pub(crate) mod v821;
pub(crate) mod v861;
