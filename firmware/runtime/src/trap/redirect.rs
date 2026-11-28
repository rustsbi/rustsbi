//! Synchronous exception forwarding to S/HS or VS.
//!
//! [Supervisor double-trap rules](https://docs.riscv.org/reference/isa/v20260120/priv/supervisor.html#supv-double-trap).
//! [Landing-pad state on trap entry and return](https://docs.riscv.org/reference/isa/v20260120/priv/priv-cfi.html#norm:Zicfilp_pelp_trap).

use crate::{Error, Result};
use csr::{
    Read, Write,
    standard::isa::{
        privileged::{
            hypervisor::{Hstatus, Htinst, Htval, read_hedeleg_bit},
            machine::Mstatus,
            supervisor::{Scause, Sepc, Stval, Stvec},
            virtual_supervisor::{Vscause, Vsepc, Vsstatus, Vstval, Vstvec},
        },
        values::{
            address::Address,
            status::{Privilege, SupervisorPrivilege},
            trap::{CauseValue, Exception, TrapCause, TrapInstructionBits, TrapValueBits},
            xlen::Xlen,
        },
    },
};

use super::{
    Fault, dispatch,
    frame::{TrapFrame, TrapInfo},
};

/// Prepares the supervisor trap state and return PC for the interrupted hart.
///
/// A double trap targeting VS is reported to HS. A double trap targeting
/// S/HS invokes the fatal M-mode handler.
///
/// # Safety
///
/// Run inside Runtime trap dispatch with its recovery ABI active, `MIE` and `MPRV`
/// clear, and the original interrupted context preserved in `frame` and `info`.
pub(super) unsafe fn redirect_trap(
    frame: &mut TrapFrame,
    info: &TrapInfo,
    privilege: SupervisorPrivilege,
    exception: Exception,
    secondary: Option<Fault>,
) -> Result<()> {
    let (mut exception, tval, mut tval2, instruction) = match secondary {
        Some(fault) => (fault.cause, fault.address.bits(), 0, 0),
        None => (exception, info.tval, info.tval2, info.instruction),
    };
    let supervisor_xlen = info
        .xlen(Privilege::Supervisor)
        .ok_or(Error::NotSupported)?;
    // SAFETY: A virtualized origin establishes H. Entry saved the return state
    // across a possible fault reading the RV32 upper word.
    if info.virtualized() && unsafe { read_hedeleg_bit(exception) }.unwrap_or(false) {
        let mut status = Vsstatus::read().into_write();
        if status.get(Vsstatus::SDT) {
            // Preserve the guest's critical trap CSRs. HS receives the original
            // exception code in htval and the double-trap exception in scause.
            tval2 = exception.code();
            exception = Exception::DoubleTrap;
        } else {
            let guest_xlen = match supervisor_xlen {
                #[cfg(target_pointer_width = "64")]
                Xlen::X64 => Hstatus::read()
                    .get(Hstatus::VSXL)
                    .map_err(|_| Error::NotSupported)?,
                _ => Xlen::X32,
            };
            let cause = CauseValue::from_cause(TrapCause::Exception(exception), guest_xlen)
                .ok_or(Error::NotSupported)?;
            status.set(Vsstatus::SPIE, status.get(Vsstatus::SIE));
            status.set(Vsstatus::SIE, false);
            status.set(Vsstatus::SPP, privilege);
            status.set(Vsstatus::SDT, true);
            status.set(Vsstatus::SPELP, frame.take_previous_landing_pad());
            // SAFETY: H is present and VS can accept this exception.
            unsafe {
                Vsepc::write(Address::from_bits(info.epc));
                Vscause::write(cause);
                Vstval::write(TrapValueBits::from_bits(tval));
                Vsstatus::write(status);
            }
            // MPV stays set so MRET enters the guest's handler.
            frame.status.set(Mstatus::MPP, Privilege::Supervisor);
            frame.epc = Vstvec::read().get(Vstvec::BASE).address().bits();
            return Ok(());
        }
    }

    if info.status.get(Mstatus::SDT) {
        dispatch::fatal_double_trap(frame, info, exception, tval, instruction);
    }
    let cause = CauseValue::from_cause(TrapCause::Exception(exception), supervisor_xlen)
        .ok_or(Error::NotSupported)?;
    if info.has_hypervisor {
        // SAFETY: Trap entry saved the H extension's origin and fault metadata.
        unsafe {
            let mut status = Hstatus::read().into_write();
            status.set(Hstatus::SPV, info.virtualized());
            if info.virtualized() {
                status.set(Hstatus::SPVP, privilege);
            }
            status.set(Hstatus::GVA, secondary.is_none() && info.guest_address());
            Hstatus::write(status);
            Htval::write(TrapValueBits::from_bits(tval2));
            Htinst::write(TrapInstructionBits::from_bits(instruction));
        }
    }
    // SAFETY: S/HS can accept this exception and local interrupts are excluded.
    unsafe {
        Sepc::write(Address::from_bits(info.epc));
        Scause::write(cause);
        Stval::write(TrapValueBits::from_bits(tval));
    }
    frame
        .status
        .set(Mstatus::SPIE, info.status.get(Mstatus::SIE));
    frame.status.set(Mstatus::SIE, false);
    frame.status.set(Mstatus::SPP, privilege);
    frame.status.set(Mstatus::SDT, true);
    let landing_pad = frame.take_previous_landing_pad();
    frame.status.set(Mstatus::SPELP, landing_pad);
    frame.status.set(Mstatus::MPP, Privilege::Supervisor);
    frame.leave_virtualization();
    frame.epc = Stvec::read().get(Stvec::BASE).address().bits();
    Ok(())
}
