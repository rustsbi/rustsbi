//! Vendor-specific SBI extensions.

pub(crate) mod allwinner;

pub(crate) use allwinner::v821::{Devices as VendorDevices, Extensions as Vendor};
