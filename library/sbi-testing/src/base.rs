//! RISC-V SBI Base extension test suite.

use sbi::{ExtensionInfo, Version};
use sbi_spec::base::impl_id;

/// Base extension test cases.
#[derive(Clone, Debug)]
pub enum Case {
    /// Can't proceed test for base extension does not exist.
    NotExist,
    /// Test begin.
    Begin,
    /// Test process for getting SBI specification version.
    GetSbiSpecVersion(Version),
    /// Test process for getting SBI implementation ID.
    GetSbiImplId(Result<&'static str, usize>),
    /// Test process for getting version of SBI implementation.
    GetSbiImplVersion(usize),
    /// Test process for probe standard SBI extensions.
    ProbeExtensions(Extensions),
    /// Test process for getting vendor ID from RISC-V environment.
    GetMvendorId(usize),
    /// Test process for getting architecture ID from RISC-V environment.
    GetMarchId(usize),
    /// Test process for getting implementation ID from RISC-V environment.
    GetMimpId(usize),
    /// All test cases on base module finished.
    Pass,
}

/// Information about all SBI standard extensions.
#[derive(Clone, Debug)]
pub struct Extensions {
    /// Timer programmer extension.
    pub time: ExtensionInfo,
    /// Inter-processor Interrupt extension.
    pub spi: ExtensionInfo,
    /// Remote Fence extension.
    pub rfnc: ExtensionInfo,
    /// Hart State Monitor extension.
    pub hsm: ExtensionInfo,
    /// System Reset extension.
    pub srst: ExtensionInfo,
    /// Performance Monitor Unit extension.
    pub pmu: ExtensionInfo,
}

impl core::fmt::Display for Extensions {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "[Base")?;
        if self.time.is_available() {
            write!(f, ", TIME")?;
        }
        if self.spi.is_available() {
            write!(f, ", sPI")?;
        }
        if self.rfnc.is_available() {
            write!(f, ", RFNC")?;
        }
        if self.hsm.is_available() {
            write!(f, ", HSM")?;
        }
        if self.srst.is_available() {
            write!(f, ", SRST")?;
        }
        if self.pmu.is_available() {
            write!(f, ", PMU")?;
        }
        write!(f, "]")
    }
}

/// Test base extension.
///
/// The test case output would be handled in `f`.
pub fn test(mut f: impl FnMut(Case)) {
    if sbi::probe_extension(sbi::Base).is_unavailable() {
        f(Case::NotExist);
        return;
    }
    f(Case::Begin);
    f(Case::GetSbiSpecVersion(sbi::get_spec_version()));
    f(Case::GetSbiImplId(match sbi::get_sbi_impl_id() {
        impl_id::BBL => Ok("BBL"),
        impl_id::OPEN_SBI => Ok("OpenSBI"),
        impl_id::XVISOR => Ok("Xvisor"),
        impl_id::KVM => Ok("KVM"),
        impl_id::RUST_SBI => Ok("RustSBI"),
        impl_id::DIOSIX => Ok("Diosix"),
        impl_id::COFFER => Ok("Coffer"),
        impl_id::XEN => Ok("Xen Project"),
        impl_id::POLARFIRE_HSS => Ok("PolarFire Hart Software Services"),
        impl_id::COREBOOT => Ok("Coreboot"),
        impl_id::OREBOOT => Ok("Oreboot"),
        unknown => Err(unknown),
    }));
    f(Case::GetSbiImplVersion(sbi::get_sbi_impl_version()));
    f(Case::ProbeExtensions(Extensions {
        time: sbi::probe_extension(sbi::Timer),
        spi: sbi::probe_extension(sbi::Ipi),
        rfnc: sbi::probe_extension(sbi::Fence),
        hsm: sbi::probe_extension(sbi::Hsm),
        srst: sbi::probe_extension(sbi::Reset),
        pmu: sbi::probe_extension(sbi::Pmu),
    }));
    f(Case::GetMvendorId(sbi::get_mvendorid()));
    f(Case::GetMarchId(sbi::get_marchid()));
    f(Case::GetMimpId(sbi::get_mimpid()));
    f(Case::Pass);
}
