//! Chapter 4. Base Extension (EID #0x10).

/// Extension ID for RISC-V SBI Base extension.
pub const EID_BASE: usize = 0x10;
pub use fid::*;

/// Default probe value for the target SBI extension is unavailable.
pub const UNAVAILABLE_EXTENSION: usize = 0;

/// SBI specification version.
///
/// Not to be confused with 'implementation version'.
///
/// Declared in §4.1.
#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
pub struct Version {
    raw: usize,
}

impl Version {
    /// Converts raw extension value into Version structure.
    #[inline]
    pub const fn from_raw(raw: usize) -> Self {
        Self { raw }
    }

    /// Reads the major version of RISC-V SBI specification.
    #[inline]
    pub const fn major(self) -> usize {
        (self.raw >> 24) & ((1 << 7) - 1)
    }

    /// Reads the minor version of RISC-V SBI specification.
    #[inline]
    pub const fn minor(self) -> usize {
        self.raw & ((1 << 24) - 1)
    }
}

impl core::fmt::Display for Version {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}.{}", self.major(), self.minor())
    }
}

/// Declared in §4.8
mod fid {
    /// Function ID to get the current SBI specification version.
    ///
    /// Declared in §4.1.
    pub const GET_SBI_SPEC_VERSION: usize = 0x0;
    /// Function ID to get the current SBI implementation ID.
    ///
    /// Declared in §4.2.
    pub const GET_SBI_IMPL_ID: usize = 0x1;
    /// Function ID to get the current SBI implementation version.
    ///
    /// Declared in §4.3.
    pub const GET_SBI_IMPL_VERSION: usize = 0x2;
    /// Function ID to probe information about one SBI extension from the current environment.
    ///
    /// Declared in §4.4.
    pub const PROBE_EXTENSION: usize = 0x3;
    /// Function ID to get the value of `mvendorid` register in the current environment.
    ///
    /// Declared in §4.5.
    pub const GET_MVENDORID: usize = 0x4;
    /// Function ID to get the value of `marchid` register in the current environment.
    ///
    /// Declared in §4.6.
    pub const GET_MARCHID: usize = 0x5;
    /// Function ID to get the value of `mimpid` register in the current environment.
    ///
    /// Declared in §4.7.
    pub const GET_MIMPID: usize = 0x6;
}

/// SBI Implementation IDs.
///
/// Declared in §4.9.
pub mod impl_id {
    /// Berkley Bootloader.
    pub const BBL: usize = 0;
    /// OpenSBI.
    pub const OPEN_SBI: usize = 1;
    /// Xvisor.
    pub const XVISOR: usize = 2;
    /// KVM.
    pub const KVM: usize = 3;
    /// RustSBI.
    pub const RUST_SBI: usize = 4;
    /// Diosix.
    pub const DIOSIX: usize = 5;
    /// Coffer.
    pub const COFFER: usize = 6;
    /// Xen Project.
    pub const XEN: usize = 7;
    /// PolarFire Hart Software Services.
    pub const POLARFIRE_HSS: usize = 8;
    /// Coreboot.
    pub const COREBOOT: usize = 9;
    /// Oreboot.
    pub const OREBOOT: usize = 10;
}
