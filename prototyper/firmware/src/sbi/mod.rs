//! SBI protocol adapters over machine capabilities.

mod console;
mod hsm;
mod ipi;
mod pmu;
mod reset;
mod rfence;
mod timer;

use machine::{Console, SbiCall};
use rustsbi::RustSBI;
use sbi_spec::{
    binary::{Error, SbiRet},
    pmu::firmware_event,
    rfnc, spi, time,
};

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
pub struct Dispatcher {
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

impl Dispatcher {
    /// Builds every SBI extension from the completed firmware capability set.
    pub(crate) fn from_capabilities(
        interrupts: Option<machine::Interrupts>,
        power: bool,
        console: Option<Console>,
        memory: machine::memory::SupervisorMemory,
        counters: Option<machine::PerformanceCounters>,
        hart_count: usize,
    ) -> Result<Self, machine::HartLocalError> {
        let (timer, ipi, fence, harts) = match interrupts {
            Some(interrupts) => (
                Some(interrupts.timer),
                Some(interrupts.ipi),
                Some(interrupts.remote_fence),
                Some(interrupts.harts),
            ),
            None => (None, None, None, None),
        };
        let pmu = match counters {
            Some(counters) => Some(PerformanceMonitor::new(counters, hart_count)?),
            None => None,
        };
        Ok(Self {
            timer: timer.map(Timer::new),
            ipi: ipi.map(Ipi::new),
            hsm: harts.map(Hsm::new),
            rfence: fence.map(Rfence::new),
            reset: power.then(Reset::new),
            console: console.map(|console| DebugConsole::new(console, memory)),
            pmu,
        })
    }

    fn record_sbi_call(&self, extension_id: usize, function_id: usize) {
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
        if let (Some(event), Some(pmu)) = (event, &self.pmu) {
            pmu.record(event);
        }
    }
}

impl machine::SbiHandler for Dispatcher {
    fn handle(&self, call: SbiCall) -> sbi_spec::binary::SbiRet {
        let result =
            RustSBI::handle_ecall(self, call.extension_id, call.function_id, call.arguments);
        self.record_sbi_call(call.extension_id, call.function_id);
        result
    }
}

fn response(result: Result<usize, Error>) -> SbiRet {
    match result {
        Ok(value) => SbiRet::success(value),
        Err(error) => error.into(),
    }
}
