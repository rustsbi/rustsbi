//! Interrupt-controller drivers.
//!
//! IMSIC and APLIC are AIA devices; the AIA module composes them into the
//! platform IPI device.

pub(crate) mod aia;
pub(crate) mod aplic;
