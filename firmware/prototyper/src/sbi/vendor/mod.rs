//! Vendor-specific SBI extension selection.
//!
//! The SBI layer owns only the dynamic [`VendorSBI`] interface; target code
//! registers its extension set without exposing its concrete type here.

use alloc::boxed::Box;

use spin::Mutex;

use runtime::{
    memory::SupervisorMemory,
    rustsbi::{SbiRet, VendorSBI},
};

/// One target-provided vendor extension set.
pub(crate) trait VendorExtension: Send {
    fn take_vendor(&mut self, memory: &'static SupervisorMemory) -> Vendor;
}

/// A registered vendor extension set.
pub(crate) struct Vendor(Box<dyn VendorSBI + Send + Sync>);

impl Vendor {
    pub(crate) fn new(extension: Box<dyn VendorSBI + Send + Sync>) -> Self {
        Self(extension)
    }
}

impl VendorSBI for Vendor {
    #[inline]
    fn probe_extension(&self, extension: usize) -> usize {
        self.0.probe_extension(extension)
    }

    #[inline]
    fn handle_ecall(&self, extension: usize, function: usize, args: [usize; 6]) -> SbiRet {
        self.0.handle_ecall(extension, function, args)
    }
}

/// Vendor extension registered by the selected target.
static EXTENSION: Mutex<Option<Box<dyn VendorExtension>>> = Mutex::new(None);

/// Publishes the target's vendor extension for the SBI dispatcher.
pub(crate) fn set_extension(extension: Box<dyn VendorExtension>) {
    *EXTENSION.lock() = Some(extension);
}

/// Takes the registered vendor extension, if the target provided one.
pub(crate) fn take_vendor(memory: &'static SupervisorMemory) -> Option<Vendor> {
    EXTENSION
        .lock()
        .as_mut()
        .map(|extension| extension.take_vendor(memory))
}
