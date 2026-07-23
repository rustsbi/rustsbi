//! Transactional construction and publication of the machine runtime.

use alloc::boxed::Box;
use alloc::sync::Arc;
use dtoolkit::fdt::Fdt;

use super::BootInfo;
use super::to_next::enter;

/// Publishes the complete machine runtime and enters the validated next stage.
///
/// This is the only successful transition out of safe upper boot policy. Every
/// fallible check completes before the Release publication that lets secondary
/// harts acquire their machine-owned stack and trap state.
pub(crate) fn enter_next_stage(boot: BootInfo, handler: Box<dyn crate::SbiHandler>) -> ! {
    let prepared = prepare_runtime(boot, handler);
    let PreparedRuntime {
        boot, init_hart, ..
    } = prepared;
    let BootInfo {
        dtb, next_stage, ..
    } = boot;
    let dtb_address = match dtb.into_handoff_address() {
        Ok(address) => address,
        Err(_) => terminal_failure(TerminalFailure::HandoffDeviceTree),
    };
    crate::startup::publish_runtime();
    enter(next_stage, init_hart, Some(dtb_address))
}

pub(crate) struct PreparedRuntime {
    pub(crate) boot: BootInfo,
    pub(crate) init_hart: usize,
    #[cfg(feature = "mtest")]
    pub(crate) hart_count: usize,
}

/// Establishes the complete machine runtime without committing a terminal
/// next-stage action.
///
/// This private typestate is shared by the production handoff and the
/// dedicated test build. Keeping publication outside this function ensures
/// that failure cannot release secondary harts into a partial runtime.
pub(crate) fn prepare_runtime(
    boot: BootInfo,
    handler: Box<dyn crate::SbiHandler>,
) -> PreparedRuntime {
    let fdt = match Fdt::new(boot.dtb.as_bytes()) {
        Ok(fdt) => fdt,
        Err(_) => terminal_failure(TerminalFailure::DeviceTree),
    };
    let hart_ids = match super::device_tree::hart_ids(&fdt) {
        Ok(ids) => ids,
        Err(_) => terminal_failure(TerminalFailure::HartDescription),
    };
    if crate::hart::publish(&hart_ids, boot.init_hart).is_err() {
        terminal_failure(TerminalFailure::HartPublication);
    }
    let init_index = match crate::hart::resolve(boot.init_hart) {
        Some(index) => index,
        None => terminal_failure(TerminalFailure::InitHart),
    };
    if hart_ids.len() > 1 && boot.hart_admission.is_none() {
        terminal_failure(TerminalFailure::MissingHartAdmission);
    }
    if boot
        .hart_admission
        .as_ref()
        .is_some_and(|admission| !admission.matches_harts(&hart_ids))
    {
        terminal_failure(TerminalFailure::HartDescription);
    }
    if let Some(admission) = boot.hart_admission.as_ref()
        && crate::hart::protocol::publish(Arc::clone(admission)).is_err()
    {
        terminal_failure(TerminalFailure::HartAdmissionPublication);
    }

    let handler: &'static dyn crate::SbiHandler = Box::leak(handler);
    if let Some(timer) = boot.timer
        && crate::timer::install(timer).is_err()
    {
        terminal_failure(TerminalFailure::TimerPreparation);
    }
    if let Some(counters) = boot.counters.as_ref()
        && crate::pmu::install(counters.share()).is_err()
    {
        terminal_failure(TerminalFailure::CounterPreparation);
    }
    if crate::trap::install(hart_ids.len(), handler).is_err() {
        terminal_failure(TerminalFailure::TrapPreparation);
    }
    if crate::trap::activate(init_index).is_err() {
        terminal_failure(TerminalFailure::TrapActivation);
    }
    if crate::trap::prepare_hypervisor_metadata().is_err() {
        terminal_failure(TerminalFailure::TrapPreparation);
    }
    if crate::trap::prepare_timer().is_err() {
        terminal_failure(TerminalFailure::TimerPreparation);
    }
    if let Some(admission) = &boot.hart_admission
        && admission.prepare_current_hart().is_err()
    {
        terminal_failure(TerminalFailure::HartAdmissionPreparation);
    }
    if crate::trap::prepare_counters(boot.next_stage.mode()).is_err() {
        terminal_failure(TerminalFailure::CounterPreparation);
    }
    let Some(protection) = boot.protection.as_ref() else {
        terminal_failure(TerminalFailure::ProtectionPublication);
    };
    if crate::pmp::publish(&boot.machine_ranges, protection).is_err() {
        terminal_failure(TerminalFailure::ProtectionPublication);
    }
    if crate::pmp::configure_current_hart().is_err() {
        terminal_failure(TerminalFailure::ProtectionInstallation);
    }
    if crate::trap::prepare_delegation(boot.next_stage.mode()).is_err() {
        terminal_failure(TerminalFailure::Delegation);
    }

    PreparedRuntime {
        init_hart: boot.init_hart,
        #[cfg(feature = "mtest")]
        hart_count: hart_ids.len(),
        boot,
    }
}

#[repr(usize)]
enum TerminalFailure {
    DeviceTree = 1,
    HartDescription,
    HartPublication,
    InitHart,
    MissingHartAdmission,
    HartAdmissionPublication,
    TrapPreparation,
    TrapActivation,
    TimerPreparation,
    HartAdmissionPreparation,
    CounterPreparation,
    ProtectionPublication,
    ProtectionInstallation,
    Delegation,
    HandoffDeviceTree,
}

fn terminal_failure(failure: TerminalFailure) -> ! {
    // Preserve one private post-mortem code in the trap-only scratch CSR. No
    // ordinary execution is reachable after this terminal transition.
    unsafe {
        core::arch::asm!(
            "csrw mscratch, {failure}",
            failure = in(reg) failure as usize,
            options(nomem, nostack),
        )
    };
    crate::startup::fail_runtime();
    crate::power::abort(|| {})
}
