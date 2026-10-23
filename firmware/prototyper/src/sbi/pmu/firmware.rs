//! Firmware event identities and their accounting entry.

use super::SbiPmu;
use crate::sbi;
use runtime::{Error, Result, trap::Exception};
use sbi_spec::pmu::firmware_event;

/// A firmware event that this implementation can observe.
#[repr(usize)]
#[derive(Clone, Copy)]
pub(crate) enum FirmwareEvent {
    MisalignedLoad = firmware_event::MISALIGNED_LOAD,
    MisalignedStore = firmware_event::MISALIGNED_STORE,
    AccessLoad = firmware_event::ACCESS_LOAD,
    AccessStore = firmware_event::ACCESS_STORE,
    IllegalInstruction = firmware_event::ILLEGAL_INSN,
    SetTimer = firmware_event::SET_TIMER,
    IpiSent = firmware_event::IPI_SENT,
    IpiReceived = firmware_event::IPI_RECEIVED,
    FenceISent = firmware_event::FENCE_I_SENT,
    FenceIReceived = firmware_event::FENCE_I_RECEIVED,
    SfenceVmaSent = firmware_event::SFENCE_VMA_SENT,
    SfenceVmaReceived = firmware_event::SFENCE_VMA_RECEIVED,
    SfenceVmaAsidSent = firmware_event::SFENCE_VMA_ASID_SENT,
    SfenceVmaAsidReceived = firmware_event::SFENCE_VMA_ASID_RECEIVED,
    #[cfg(feature = "hypervisor")]
    HfenceGvmaSent = firmware_event::HFENCE_GVMA_SENT,
    #[cfg(feature = "hypervisor")]
    HfenceGvmaReceived = firmware_event::HFENCE_GVMA_RECEIVED,
    #[cfg(feature = "hypervisor")]
    HfenceGvmaVmidSent = firmware_event::HFENCE_GVMA_VMID_SENT,
    #[cfg(feature = "hypervisor")]
    HfenceGvmaVmidReceived = firmware_event::HFENCE_GVMA_VMID_RECEIVED,
    #[cfg(feature = "hypervisor")]
    HfenceVvmaSent = firmware_event::HFENCE_VVMA_SENT,
    #[cfg(feature = "hypervisor")]
    HfenceVvmaReceived = firmware_event::HFENCE_VVMA_RECEIVED,
    #[cfg(feature = "hypervisor")]
    HfenceVvmaAsidSent = firmware_event::HFENCE_VVMA_ASID_SENT,
    #[cfg(feature = "hypervisor")]
    HfenceVvmaAsidReceived = firmware_event::HFENCE_VVMA_ASID_RECEIVED,
}

impl TryFrom<Exception> for FirmwareEvent {
    type Error = Error;

    fn try_from(exception: Exception) -> Result<Self> {
        match exception {
            Exception::IllegalInstruction => Ok(Self::IllegalInstruction),
            Exception::LoadMisaligned => Ok(Self::MisalignedLoad),
            Exception::StoreMisaligned => Ok(Self::MisalignedStore),
            Exception::LoadAccess => Ok(Self::AccessLoad),
            Exception::StoreAccess => Ok(Self::AccessStore),
            _ => Err(Error::NotSupported),
        }
    }
}

impl SbiPmu {
    /// Accounts for one typed event when the published PMU is ready.
    pub(crate) fn record_event(event: FirmwareEvent) {
        let Some(pmu) = sbi::pmu() else {
            return;
        };
        match pmu.with_current(|_, counters| {
            counters.record(event.into());
            Ok(())
        }) {
            Ok(()) | Err(Error::NotReady) => {}
            Err(error) => panic!("PMU event accounting failed: {error}"),
        }
    }
}
