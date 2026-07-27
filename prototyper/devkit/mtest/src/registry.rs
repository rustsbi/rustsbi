//! Linker-owned registry for one isolated QEMU M-test case.

use core::mem::{align_of, size_of};
use core::{slice, str};

/// Immutable linker entry emitted by `#[mtest]`.
#[repr(C)]
pub struct Descriptor {
    name: *const u8,
    name_len: usize,
    test: extern "C" fn(),
}

// SAFETY: entries name immutable literals and functions in the same image.
unsafe impl Sync for Descriptor {}

impl Descriptor {
    /// Creates one statically linked test descriptor.
    pub const fn new(name: &'static str, test: extern "C" fn()) -> Self {
        Self {
            name: name.as_ptr(),
            name_len: name.len(),
            test,
        }
    }

    fn name(&self) -> Result<&'static str, ()> {
        if self.name.is_null() || self.name_len == 0 {
            return Err(());
        }
        // SAFETY: macro expansion supplies an immutable string literal and
        // this registry validates the linker-owned descriptor envelope first.
        let bytes = unsafe { slice::from_raw_parts(self.name, self.name_len) };
        str::from_utf8(bytes).map_err(|_| ())
    }
}

/// Validated immutable registry of test descriptors.
pub struct Registry {
    descriptors: &'static [Descriptor],
}

/// A single exact case selected from a validated registry.
pub struct Selected<'registry>(&'registry Descriptor);

impl Selected<'_> {
    /// Returns the stable case name.
    pub fn name(&self) -> &'static str {
        self.0
            .name()
            .expect("registry validation checked every name")
    }

    /// Runs only this case.
    pub fn run(self) {
        (self.0.test)()
    }
}

impl Registry {
    /// Visits each registered case name.
    pub fn visit(&self, mut visitor: impl FnMut(&'static str)) {
        for descriptor in self.descriptors {
            visitor(
                descriptor
                    .name()
                    .expect("registry validation checked every name"),
            );
        }
    }

    /// Selects one exact registered case.
    pub fn select(&self, name: &str) -> Option<Selected<'_>> {
        self.descriptors
            .iter()
            .find(|descriptor| descriptor.name().is_ok_and(|candidate| candidate == name))
            .map(Selected)
    }
}

/// Validates the linker section retained by the QEMU-virt M-test script.
pub fn from_linker_bounds() -> Result<Registry, ()> {
    unsafe extern "C" {
        static __mtest_start: u8;
        static __mtest_end: u8;
    }
    let start = core::ptr::addr_of!(__mtest_start) as usize;
    let end = core::ptr::addr_of!(__mtest_end) as usize;
    let bytes = end.checked_sub(start).ok_or(())?;
    if bytes == 0
        || !start.is_multiple_of(align_of::<Descriptor>())
        || !bytes.is_multiple_of(size_of::<Descriptor>())
    {
        return Err(());
    }
    // SAFETY: linker assertions restrict this section to complete descriptors;
    // alignment and exact size divisibility were checked above.
    let descriptors = unsafe {
        slice::from_raw_parts(start as *const Descriptor, bytes / size_of::<Descriptor>())
    };
    for (index, descriptor) in descriptors.iter().enumerate() {
        let name = descriptor.name()?;
        if descriptors[..index]
            .iter()
            .any(|previous| previous.name() == Ok(name))
        {
            return Err(());
        }
    }
    Ok(Registry { descriptors })
}
