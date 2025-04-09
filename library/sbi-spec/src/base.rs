//! Chapter 4. Base Extension (EID #0x10).

/// Extension ID for RISC-V SBI Base extension.
#[doc(alias = "SBI_EXT_BASE")]
pub const EID_BASE: usize = 0x10;
pub use fid::*;

/// Default probe value for the target SBI extension is unavailable.
pub const UNAVAILABLE_EXTENSION: usize = 0;

/// SBI specification version.
///
/// In RISC-V SBI specification, the bit 31 must be 0 and is reserved for future expansion.
///
/// Not to be confused with 'implementation version'.
///
/// Declared in §4.1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Version {
    raw: usize,
}

impl Version {
    /// RISC-V SBI version 1.0, ratified at Mar 23, 2022.
    pub const V1_0: Version = Version::from_raw(0x0100_0000);

    /// RISC-V SBI version 2.0, ratified at Feb 1, 2024.
    pub const V2_0: Version = Version::from_raw(0x0200_0000);

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

impl core::cmp::PartialOrd for Version {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl core::cmp::Ord for Version {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.major()
            .cmp(&other.major())
            .then_with(|| self.minor().cmp(&other.minor()))
    }
}

/// Declared in §4.8
mod fid {
    /// Function ID to get the current SBI specification version.
    ///
    /// Declared in §4.1.
    #[doc(alias = "SBI_EXT_BASE_GET_SPEC_VERSION")]
    pub const GET_SBI_SPEC_VERSION: usize = 0x0;
    /// Function ID to get the current SBI implementation ID.
    ///
    /// Declared in §4.2.
    #[doc(alias = "SBI_EXT_BASE_GET_IMP_ID")]
    pub const GET_SBI_IMPL_ID: usize = 0x1;
    /// Function ID to get the current SBI implementation version.
    ///
    /// Declared in §4.3.
    #[doc(alias = "SBI_EXT_BASE_GET_IMP_VERSION")]
    pub const GET_SBI_IMPL_VERSION: usize = 0x2;
    /// Function ID to probe information about one SBI extension from the current environment.
    ///
    /// Declared in §4.4.
    #[doc(alias = "SBI_EXT_BASE_PROBE_EXT")]
    pub const PROBE_EXTENSION: usize = 0x3;
    /// Function ID to get the value of `mvendorid` register in the current environment.
    ///
    /// Declared in §4.5.
    #[doc(alias = "SBI_EXT_BASE_GET_MVENDORID")]
    pub const GET_MVENDORID: usize = 0x4;
    /// Function ID to get the value of `marchid` register in the current environment.
    ///
    /// Declared in §4.6.
    #[doc(alias = "SBI_EXT_BASE_GET_MARCHID")]
    pub const GET_MARCHID: usize = 0x5;
    /// Function ID to get the value of `mimpid` register in the current environment.
    ///
    /// Declared in §4.7.
    #[doc(alias = "SBI_EXT_BASE_GET_MIMPID")]
    pub const GET_MIMPID: usize = 0x6;
}

/// SBI Implementation IDs.
///
/// Declared in §4.9.
pub mod impl_id {
    /// Berkeley Bootloader.
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

#[cfg(test)]
mod tests {
    use super::Version;

    #[test]
    fn version_parse() {
        let v1_0 = Version::from_raw(0x100_0000);
        assert_eq!(v1_0.major(), 1);
        assert_eq!(v1_0.minor(), 0);

        let v2_0 = Version::from_raw(0x200_0000);
        assert_eq!(v2_0.major(), 2);
        assert_eq!(v2_0.minor(), 0);

        let v2_1 = Version::from_raw(0x200_0001);
        assert_eq!(v2_1.major(), 2);
        assert_eq!(v2_1.minor(), 1);

        let v2_max = Version::from_raw(0x2ff_ffff);
        assert_eq!(v2_max.major(), 2);
        assert_eq!(v2_max.minor(), 16777215);

        let vmax_3 = Version::from_raw(0x7f00_0003);
        assert_eq!(vmax_3.major(), 127);
        assert_eq!(vmax_3.minor(), 3);

        let vmax_max = Version::from_raw(0x7fff_ffff);
        assert_eq!(vmax_max.major(), 127);
        assert_eq!(vmax_max.minor(), 16777215);
    }

    #[test]
    fn version_display() {
        extern crate alloc;
        use alloc::string::ToString;

        assert_eq!("0.0", &Version::from_raw(0).to_string());
        assert_eq!("0.1", &Version::from_raw(0x1).to_string());
        assert_eq!("1.0", &Version::from_raw(0x100_0000).to_string());
        assert_eq!("1.1", &Version::from_raw(0x100_0001).to_string());
        assert_eq!("2.0", &Version::from_raw(0x200_0000).to_string());
        assert_eq!("127.0", &Version::from_raw(0x7f00_0000).to_string());
        assert_eq!("2.16777215", &Version::from_raw(0x2ff_ffff).to_string());
        assert_eq!("127.16777215", &Version::from_raw(0x7fff_ffff).to_string());
    }

    #[test]
    fn version_ordering() {
        use core::cmp::Ordering;
        let v0_0 = Version::from_raw(0x0);
        let v0_3 = Version::from_raw(0x3);
        let v1_0 = Version::from_raw(0x100_0000);
        let v2_0 = Version::from_raw(0x200_0000);
        let v2_1 = Version::from_raw(0x200_0001);
        let v2_max = Version::from_raw(0x2ff_ffff);
        let vmax_3 = Version::from_raw(0x7f00_0003);
        let vmax_max = Version::from_raw(0x7fff_ffff);

        assert!(v0_3 != v0_0);
        assert!(!(v0_3 == v0_0));
        assert!(v0_0 == v0_0);
        assert!(vmax_max == vmax_max);

        assert!(v0_3 > v0_0);
        assert!(v0_3 >= v0_0);
        assert!(v0_0 < v0_3);
        assert!(v0_0 <= v0_3);
        assert!(v0_0 >= v0_0);
        assert!(v0_0 <= v0_0);

        assert!(v0_3 > v0_0);
        assert!(v1_0 > v0_3);
        assert!(v2_0 > v1_0);
        assert!(v2_1 > v2_0);
        assert!(v2_max > v2_1);
        assert!(vmax_3 > v2_max);
        assert!(vmax_max > vmax_3);

        assert_eq!(Version::partial_cmp(&v1_0, &v0_0), Some(Ordering::Greater));
        assert_eq!(Version::partial_cmp(&v0_0, &v1_0), Some(Ordering::Less));
        assert_eq!(Version::partial_cmp(&v0_0, &v0_0), Some(Ordering::Equal));

        assert_eq!(Version::max(v0_0, v0_0), v0_0);
        assert_eq!(Version::max(v1_0, v0_0), v1_0);
        assert_eq!(Version::max(v0_0, v1_0), v1_0);
        assert_eq!(Version::min(v0_0, v0_0), v0_0);
        assert_eq!(Version::min(v1_0, v0_0), v0_0);
        assert_eq!(Version::min(v0_0, v1_0), v0_0);

        assert_eq!(v0_0.clamp(v0_3, v2_0), v0_3);
        assert_eq!(v0_3.clamp(v0_3, v2_0), v0_3);
        assert_eq!(v1_0.clamp(v0_3, v2_0), v1_0);
        assert_eq!(v2_0.clamp(v0_3, v2_0), v2_0);
        assert_eq!(v2_1.clamp(v0_3, v2_0), v2_0);
    }

    #[test]
    fn special_versions() {
        assert_eq!(Version::V1_0.major(), 1);
        assert_eq!(Version::V1_0.minor(), 0);
        assert_eq!(Version::V2_0.major(), 2);
        assert_eq!(Version::V2_0.minor(), 0);
    }
}
