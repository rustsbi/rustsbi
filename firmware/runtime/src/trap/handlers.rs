//! SBI and machine interrupt handlers selected by trap dispatch.
use super::{
    Error,
    frame::{TrapFrame, TrapInfo},
};
use crate::rustsbi::SbiRet;
use crate::{
    boot::{self, NextStage},
    hart::{self, HartEvent},
    instruction,
    ipi::{self, Ipi},
    irq::LocalIrqDisabled,
};
use riscv_csr_spec::standard::isa::values::{status::Privilege, trap::Interrupt, xlen::Xlen};

// The firmware supplies this symbol through `#[runtime::sbi_policy]`.
// Both crates use the same Rust ABI and SBI types in the final firmware.
unsafe extern "Rust" {
    fn __rustsbi_runtime_handle_ecall(
        extension: usize,
        function: usize,
        args: [usize; 6],
    ) -> SbiRet;
}

pub(super) fn sbi_ecall(frame: &mut TrapFrame, info: &TrapInfo) -> Result<(), Error> {
    let xlen = info.xlen(Privilege::Supervisor).ok_or(Error::Unsupported)?;
    let regs = &mut frame.general;
    let args = [regs.a0, regs.a1, regs.a2, regs.a3, regs.a4, regs.a5];
    let args = if xlen != Xlen::NATIVE {
        native_args_from_rv32(regs.a7, regs.a6, args)
    } else {
        args
    };
    let hart = hart::current_hart();
    // SAFETY: The firmware's `sbi_policy` symbol has the declared ABI and
    // handles ecalls only after its policy state has been initialized.
    let ret = unsafe { __rustsbi_runtime_handle_ecall(regs.a7, regs.a6, args) };
    (regs.a0, regs.a1) = match xlen {
        Xlen::X32 => (ret.error as i32 as usize, ret.value as i32 as usize),
        Xlen::X64 => (ret.error, ret.value),
    };
    frame.advance_pc(4, xlen);

    if let Some(hart::ControlTransfer::NonRetentiveResume(next)) = hart::take_control_transfer(hart)
    {
        enter_next_stage(frame, next);
    }
    Ok(())
}

/// Converts an RV32 SBI call's arguments to the RV64 policy's register widths.
///
/// The [SBI binary encoding](https://docs.riscv.org/reference/sbi/v3.0/binary-encoding.html)
/// places the low word of a double-width value first. Individual extensions
/// define which all-ones arguments must remain all ones after widening.
fn native_args_from_rv32(extension: usize, function: usize, args: [usize; 6]) -> [usize; 6] {
    use crate::rustsbi::spec::{cppc, dbcn, dbtr, mpxy, nacl, pmu, rfnc, spi, sse, sta, time};

    const ALL: usize = u32::MAX as usize;
    let mut args = args.map(|word| word as u32 as usize);
    let pair = |i: usize| (args[i] as u64 | ((args[i + 1] as u64) << 32)) as usize;
    match (extension, function) {
        (time::EID_TIME, time::SET_TIMER) => args[0] = pair(0),
        (cppc::EID_CPPC, cppc::WRITE) => args[1] = pair(1),
        (pmu::EID_PMU, pmu::COUNTER_CONFIG_MATCHING) => args[4] = pair(4),
        (pmu::EID_PMU, pmu::COUNTER_START) => args[3] = pair(3),
        (pmu::EID_PMU, pmu::SNAPSHOT_SET_SHMEM)
        | (sta::EID_STA, sta::SET_SHMEM)
        | (dbtr::EID_DBTR, dbtr::SET_SHMEM)
        | (mpxy::EID_MPXY, mpxy::SET_SHMEM)
        | (nacl::EID_NACL, nacl::SET_SHMEM) => {
            let address = if args[..2] == [ALL, ALL] {
                [usize::MAX; 2]
            } else {
                [pair(0), 0]
            };
            args[..2].copy_from_slice(&address);
        }
        (pmu::EID_PMU, pmu::EVENT_GET_INFO) => {
            args[0] = pair(0);
            args[1] = 0;
        }
        (dbcn::EID_DBCN, dbcn::CONSOLE_READ | dbcn::CONSOLE_WRITE) => {
            args[1] = pair(1);
            args[2] = 0;
        }
        (sse::EID_SSE, sse::READ_ATTRS | sse::WRITE_ATTRS) => {
            args[3] = pair(3);
            args[4] = 0;
        }
        (spi::EID_SPI, spi::SEND_IPI) | (rfnc::EID_RFNC, _) => {
            if args[1] == ALL {
                args[1] = usize::MAX;
            }
            if extension == rfnc::EID_RFNC && args[3] == ALL {
                args[3] = usize::MAX;
            }
        }
        (nacl::EID_NACL, nacl::SYNC_CSR | nacl::SYNC_HFENCE) if args[0] == ALL => {
            args[0] = usize::MAX;
        }
        _ => {}
    }
    args
}

fn deliver_event(frame: &mut TrapFrame, ipi: Option<&Ipi>, irq: &LocalIrqDisabled) {
    let event = hart::take_local_event();
    if let Some(ipi) = ipi {
        ipi.process_pending(irq);
    }
    match event {
        HartEvent::Start(next) => enter_next_stage(frame, next),
        HartEvent::Park => {
            if let Some(ipi) = ipi {
                ipi.enable_interrupt(irq);
            }
            instruction::wait_for_interrupt();
        }
        HartEvent::None => {}
    }
}

pub(super) fn software_interrupt(
    frame: &mut TrapFrame,
    irq: &LocalIrqDisabled,
) -> crate::Result<()> {
    let ipi = match ipi::get() {
        Some(device) if device.acknowledge_source(Interrupt::MachineSoftware, irq)? => Some(device),
        _ => None,
    };
    deliver_event(frame, ipi, irq);
    Ok(())
}

pub(super) fn external_interrupt(
    frame: &mut TrapFrame,
    irq: &LocalIrqDisabled,
) -> crate::Result<()> {
    if let Some(device) = ipi::get()
        && device.acknowledge_source(Interrupt::MachineExternal, irq)?
    {
        deliver_event(frame, Some(device), irq);
    }
    Ok(())
}

fn enter_next_stage(frame: &mut TrapFrame, next: NextStage) {
    // SAFETY: Dispatch owns this hart's trap state, with MIE masked.
    unsafe { boot::prepare_supervisor() };
    boot::set_return_mode(&mut frame.status, next.next_mode);
    frame.leave_virtualization();
    // A new entry point has no pending landing-pad check from the prior context.
    frame.take_previous_landing_pad();
    frame.epc = next.start_addr;
    frame.general = Default::default();
    frame.general.a0 = hart::current_hart().as_usize();
    frame.general.a1 = next.opaque;
    instruction::fence_i();
}
