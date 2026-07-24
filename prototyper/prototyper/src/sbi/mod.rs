//! SBI protocol adapters over machine capabilities.

mod console;
mod hsm;
mod ipi;
mod pmu;
mod reset;
mod rfence;
mod timer;

use machine::{
    Console, HartControl, Ipi as MachineIpi, RemoteFence as MachineRemoteFence, SbiCall,
    Timer as MachineTimer,
};
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
    /// Creates an SBI dispatcher with no optional extensions installed.
    pub const fn new() -> Self {
        Self {
            timer: None,
            ipi: None,
            hsm: None,
            rfence: None,
            reset: None,
            console: None,
            pmu: None,
        }
    }

    /// Attaches the optional TIME extension capability.
    pub fn timer(mut self, timer: Option<MachineTimer>) -> Self {
        self.timer = timer.map(Timer::new);
        self
    }

    /// Attaches the optional IPI extension capability.
    pub fn ipi(mut self, ipi: Option<MachineIpi>) -> Self {
        self.ipi = ipi.map(Ipi::new);
        self
    }

    /// Attaches the optional HSM and system-suspend capability.
    pub fn hart_control(mut self, harts: Option<HartControl>) -> Self {
        self.hsm = harts.map(Hsm::new);
        self
    }

    /// Attaches the optional remote-fence capability.
    pub fn remote_fence(mut self, fence: Option<MachineRemoteFence>) -> Self {
        self.rfence = fence.map(Rfence::new);
        self
    }

    /// Advertises SRST only when a whole-machine power provider is installed.
    pub fn system_reset(mut self, available: bool) -> Self {
        self.reset = available.then(Reset::new);
        self
    }

    /// Attaches DBCN only when the console and supervisor-memory view exist.
    pub fn debug_console(
        mut self,
        console: Option<Console>,
        memory: machine::memory::SupervisorMemory,
    ) -> Self {
        self.console = console.map(|console| DebugConsole::new(console, memory));
        self
    }

    /// Attaches an already prepared SBI performance-monitoring service.
    pub(crate) fn performance_monitor(mut self, monitor: Option<PerformanceMonitor>) -> Self {
        self.pmu = monitor;
        self
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

pub(crate) fn prepare_performance_monitor(
    counters: Option<machine::PerformanceCounters>,
    hart_count: usize,
) -> Result<Option<PerformanceMonitor>, machine::HartLocalError> {
    counters
        .map(|counters| PerformanceMonitor::new(counters, hart_count))
        .transpose()
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
