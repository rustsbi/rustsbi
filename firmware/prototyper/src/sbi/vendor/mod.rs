//! Vendor-specific SBI extension selection.

pub(crate) mod allwinner;

use runtime::memory::{MemoryRegistry, SupervisorMemory};
use runtime::rustsbi::VendorSBI;

use crate::platform::allwinner::v821::V821;

/// Vendor-extension devices selected for this platform.
pub(crate) struct Extension {
    v821: Option<allwinner::v821::Extension>,
}

impl Extension {
    pub(crate) fn bind(v821: Option<V821>, memory: &mut MemoryRegistry) -> runtime::Result<Self> {
        let v821 = v821
            .map(|platform| allwinner::v821::Extension::bind(platform, memory))
            .transpose()?;
        Ok(Self { v821 })
    }
}

/// Vendor-specific extension set selected for the current platform.
#[derive(VendorSBI)]
#[rustsbi(crate = runtime::rustsbi)]
pub(crate) enum Vendor {
    Allwinner(allwinner::v821::Extensions),
}

impl Vendor {
    pub(crate) fn new(extension: Extension, memory: &'static SupervisorMemory) -> Option<Self> {
        extension
            .v821
            .map(|extension| Self::Allwinner(extension.into_extensions(memory)))
    }
}
