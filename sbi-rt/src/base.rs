//! Chapter 4. Base Extension (EID #0x10)

use crate::binary::{sbi_call_0, sbi_call_1};

use sbi_spec::base::{
    Version, EID_BASE, GET_MARCHID, GET_MIMPID, GET_MVENDORID, GET_SBI_IMPL_ID,
    GET_SBI_IMPL_VERSION, GET_SBI_SPEC_VERSION, PROBE_EXTENSION,
};

/// Return the current SBI specification version.
///
/// The minor number of the SBI specification is encoded in the low 24 bits,
/// with the major number encoded in the next 7 bits.
/// Bit 31 must be zero and is reserved for future expansion.
///
/// This function is defined in RISC-V SBI Specification chapter 4.1.
/// According to the introduction of chapter 4, all base extension functions
/// must success and return no error code.
#[inline]
pub fn get_spec_version() -> Version {
    Version::from_raw(sbi_call_0(EID_BASE, GET_SBI_SPEC_VERSION).value)
}

/// Return the current SBI implementation ID.
///
/// Implementation ID is different for every SBI implementation.
/// It is intended that this implementation ID allows software to probe
/// for SBI implementation quirks.
///
/// This function is defined in RISC-V SBI Specification chapter 4.2.
/// According to the introduction of chapter 4, all base extension functions
/// must success and return no error code.
#[inline]
pub fn get_sbi_impl_id() -> usize {
    sbi_call_0(EID_BASE, GET_SBI_IMPL_ID).value
}

/// Return the current SBI implementation version.
///
/// The encoding of this version number is specific to the SBI implementation.
///
/// This function is defined in RISC-V SBI Specification chapter 4.3.
/// According to the introduction of chapter 4, all base extension functions
/// must success and return no error code.
#[inline]
pub fn get_sbi_impl_version() -> usize {
    sbi_call_0(EID_BASE, GET_SBI_IMPL_VERSION).value
}

/// Probe information about one SBI extension from the current environment.
///
/// Returns 0 if given SBI `extension_id` is not available, or typically
/// 1 if it's available. Implementation would define further non-zero
/// return values for information about this extension if it is available.
///
/// This function is defined in RISC-V SBI Specification chapter 4.4.
/// According to the introduction of chapter 4, all base extension functions
/// must success and return no error code.
#[inline]
pub fn probe_extension<E>(extension: E) -> ExtensionInfo
where
    E: Extension,
{
    let ans = sbi_call_1(EID_BASE, PROBE_EXTENSION, extension.extension_id());
    ExtensionInfo { raw: ans.value }
}

/// Return the value of `mvendorid` register in the current environment.
///
/// This function returns a value that is legal for the `mvendorid` register,
/// and 0 is always a legal value for this register.
///
/// This function is defined in RISC-V SBI Specification chapter 4.5.
/// According to the introduction of chapter 4, all base extension functions
/// must success and return no error code.
#[inline]
pub fn get_mvendorid() -> usize {
    sbi_call_0(EID_BASE, GET_MVENDORID).value
}

/// Return value of `marchid` register in the current environment.
///
/// This function returns a value that is legal for the `marchid` register,
/// and 0 is always a legal value for this register.
///
/// This function is defined in RISC-V SBI Specification chapter 4.6.
/// According to the introduction of chapter 4, all base extension functions
/// must success and return no error code.
#[inline]
pub fn get_marchid() -> usize {
    sbi_call_0(EID_BASE, GET_MARCHID).value
}

/// Return value of `mimpid` register in the current environment.
///
/// This function returns a value that is legal for the `mimpid` register,
/// and 0 is always a legal value for this register.
///
/// This function is defined in RISC-V SBI Specification chapter 4.7.
/// According to the introduction of chapter 4, all base extension functions
/// must success and return no error code.
#[inline]
pub fn get_mimpid() -> usize {
    sbi_call_0(EID_BASE, GET_MIMPID).value
}

/// An SBI extension.
pub trait Extension {
    /// Get a raw `extension_id` value to pass to SBI environment.
    fn extension_id(&self) -> usize;
}

macro_rules! define_extension {
    ($($struct:ident($value:expr) #[$doc:meta])*) => {
        $(
            #[derive(Clone, Copy, Debug)]
            #[$doc]
            pub struct $struct;
            impl Extension for $struct {
                #[inline]
                fn extension_id(&self) -> usize {
                    $value
                }
            }
        )*
    };
}

define_extension! {
    Base(sbi_spec::base::EID_BASE) /// RISC-V SBI Base extension.
    Timer(sbi_spec::time::EID_TIME) /// Timer programmer extension.
    Ipi(sbi_spec::spi::EID_SPI) /// Inter-processor Interrupt extension.
    Fence(sbi_spec::rfnc::EID_RFNC) /// Remote Fence extension.
    Hsm(sbi_spec::hsm::EID_HSM) /// Hart State Monitor extension.
    Reset(sbi_spec::srst::EID_SRST) /// System Reset extension.
    Pmu(sbi_spec::pmu::EID_PMU) /// Performance Monitoring Unit extension.
    Console(sbi_spec::dbcn::EID_DBCN) /// Debug Console extension.
    Suspend(sbi_spec::susp::SUSPEND) /// System Suspend extension.
    Cppc(sbi_spec::cppc::EID_CPPC) /// SBI CPPC extension.
    Nacl(sbi_spec::nacl::EID_NACL) /// Nested Acceleration extension.
    Sta(sbi_spec::sta::EID_STA) /// Steal-time Accounting extension.
}

#[cfg(feature = "integer-impls")]
impl Extension for usize {
    #[inline]
    fn extension_id(&self) -> usize {
        *self
    }
}

#[cfg(feature = "integer-impls")]
impl Extension for isize {
    #[inline]
    fn extension_id(&self) -> usize {
        usize::from_ne_bytes(isize::to_ne_bytes(*self))
    }
}

/// Information about an SBI extension.
#[derive(Clone, Copy, Debug)]
pub struct ExtensionInfo {
    pub raw: usize,
}

impl ExtensionInfo {
    /// Is this extension available?
    #[inline]
    pub const fn is_available(&self) -> bool {
        self.raw != 0
    }

    /// Is this extension not available?
    #[inline]
    pub const fn is_unavailable(&self) -> bool {
        self.raw == 0
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn extension_id_defined() {
        use crate::Extension;
        assert_eq!(crate::Base.extension_id(), 0x10);
        assert_eq!(crate::Timer.extension_id(), 0x54494D45);
        assert_eq!(crate::Ipi.extension_id(), 0x735049);
        assert_eq!(crate::Fence.extension_id(), 0x52464E43);
        assert_eq!(crate::Hsm.extension_id(), 0x48534D);
        assert_eq!(crate::Reset.extension_id(), 0x53525354);
        assert_eq!(crate::Pmu.extension_id(), 0x504D55);
        assert_eq!(crate::Console.extension_id(), 0x4442434E);
        assert_eq!(crate::Suspend.extension_id(), 0x53555350);
        assert_eq!(crate::Cppc.extension_id(), 0x43505043);
        assert_eq!(crate::Nacl.extension_id(), 0x4E41434C);
        assert_eq!(crate::Sta.extension_id(), 0x535441);
    }
}
