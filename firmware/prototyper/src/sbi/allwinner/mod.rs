//! Allwinner custom SBI extension composition.
//!
//! This module is the only layer that binds Allwinner devices to their custom
//! SBI adapters. Generic boot code carries one opaque [`BoundExtensions`]
//! value instead of individual custom-extension devices.

use runtime::memory::{MemoryRegistry, SupervisorMemory};

use crate::platform::allwinner::v821::V821;

pub(crate) mod v821;

/// Bound devices waiting for the supervisor-memory service to be published.
pub(crate) struct BoundExtensions {
    v821: Option<v821::BoundExtensions>,
}

impl BoundExtensions {
    /// Binds every discovered Allwinner custom-extension device.
    pub(crate) fn bind(v821: Option<V821>, memory: &mut MemoryRegistry) -> runtime::Result<Self> {
        let v821 = v821
            .map(|platform| v821::BoundExtensions::bind(platform, memory))
            .transpose()?;
        Ok(Self { v821 })
    }

    /// Constructs the SBI adapters after supervisor memory becomes static.
    pub(crate) fn into_extensions(self, memory: &'static SupervisorMemory) -> Extensions {
        let v821 = self
            .v821
            .map(|extensions| extensions.into_extensions(memory))
            .unwrap_or_default();
        Extensions {
            andes: v821.andes,
            awbase: v821.awbase,
        }
    }
}

/// Allwinner fields installed into the platform SBI dispatcher.
pub(crate) struct Extensions {
    pub(crate) andes: Option<v821::Andes>,
    pub(crate) awbase: Option<v821::Awbase>,
}
