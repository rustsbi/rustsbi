//! SBI protocol adapters over machine capabilities.

mod console;
mod hsm;
mod ipi;
mod pmu;
mod reset;
mod rfence;
mod timer;

use machine::{
    Console, HartControl, Ipi as MachineIpi, Power, RemoteFence as MachineRemoteFence, SModeMemory,
    Timer as MachineTimer,
};
use rustsbi::RustSBI;
use sbi_spec::{pmu::firmware_event, rfnc, spi, time};

use self::console::DebugConsole;
use self::hsm::Hsm;
use self::ipi::Ipi;
use self::pmu::PerformanceMonitor;
use self::reset::Reset;
use self::rfence::Rfence;
use self::timer::Timer;

/// Complete upper-level SBI protocol handler.
///
/// Every field is a protocol adapter, not a physical device. Optional fields
/// make extension probing agree with the capabilities constructed at boot.
#[derive(RustSBI)]
#[rustsbi(dynamic)]
pub struct Handler {
    #[rustsbi(timer)]
    timer: Option<Timer>,
    #[rustsbi(ipi)]
    ipi: Option<Ipi>,
    #[rustsbi(hsm, susp)]
    hsm: Option<Hsm>,
    #[rustsbi(fence)]
    rfence: Option<Rfence>,
    #[rustsbi(reset)]
    reset: Option<Reset>,
    #[rustsbi(console)]
    console: Option<DebugConsole>,
    #[rustsbi(pmu)]
    pmu: Option<PerformanceMonitor>,
}

impl Handler {
    /// Builds protocol adapters from already validated machine capabilities.
    #[expect(
        clippy::too_many_arguments,
        reason = "explicit independent capabilities keep boot wiring visible and avoid a service-locator aggregate"
    )]
    pub fn new(
        timer: Option<MachineTimer>,
        ipi: Option<MachineIpi>,
        hsm: Option<HartControl>,
        rfence: Option<MachineRemoteFence>,
        power: Option<Power>,
        console: Option<Console>,
        memory: SModeMemory,
        counters: Option<machine::PerformanceCounters>,
        hart_count: usize,
    ) -> Self {
        Self {
            timer: timer.map(Timer::new),
            ipi: ipi.map(Ipi::new),
            hsm: hsm.map(Hsm::new),
            rfence: rfence.map(Rfence::new),
            reset: power.map(Reset::new),
            console: console.map(|console| DebugConsole::new(console, memory)),
            pmu: counters.map(|counters| {
                PerformanceMonitor::new(counters, hart_count)
                    .unwrap_or_else(|_| machine::abort(|| {}))
            }),
        }
    }

    pub(crate) fn record_firmware_event(&self, event: usize) {
        if let Some(pmu) = &self.pmu {
            pmu.record(event);
        }
    }

    pub(crate) fn record_sbi_call(&self, extension_id: usize, function_id: usize) {
        let event = match (extension_id, function_id) {
            (time::EID_TIME, time::SET_TIMER) => Some(firmware_event::SET_TIMER),
            (spi::EID_SPI, spi::SEND_IPI) => Some(firmware_event::IPI_SENT),
            (rfnc::EID_RFNC, rfnc::REMOTE_FENCE_I) => Some(firmware_event::FENCE_I_SENT),
            (rfnc::EID_RFNC, rfnc::REMOTE_SFENCE_VMA) => Some(firmware_event::SFENCE_VMA_SENT),
            (rfnc::EID_RFNC, rfnc::REMOTE_SFENCE_VMA_ASID) => {
                Some(firmware_event::SFENCE_VMA_ASID_SENT)
            }
            #[cfg(feature = "hypervisor")]
            (rfnc::EID_RFNC, rfnc::REMOTE_HFENCE_GVMA) => Some(firmware_event::HFENCE_GVMA_SENT),
            #[cfg(feature = "hypervisor")]
            (rfnc::EID_RFNC, rfnc::REMOTE_HFENCE_GVMA_VMID) => {
                Some(firmware_event::HFENCE_GVMA_VMID_SENT)
            }
            #[cfg(feature = "hypervisor")]
            (rfnc::EID_RFNC, rfnc::REMOTE_HFENCE_VVMA) => Some(firmware_event::HFENCE_VVMA_SENT),
            #[cfg(feature = "hypervisor")]
            (rfnc::EID_RFNC, rfnc::REMOTE_HFENCE_VVMA_ASID) => {
                Some(firmware_event::HFENCE_VVMA_ASID_SENT)
            }
            _ => None,
        };
        if let Some(event) = event {
            self.record_firmware_event(event);
        }
    }
}
