//! Forward the trap being handled to the supervisor (OpenSBI
//! `sbi_trap_redirect`).

use riscv::register::{mcause, mepc, mstatus, mtval, scause, sepc, sstatus, stval, stvec};

use super::Error;

const SSTATUS_SIE: usize = 1 << 1;
const SSTATUS_SPIE: usize = 1 << 5;

/// Forward the current trap to the supervisor: fill `sepc`/`scause`/`stval`
/// with the trap facts, derive `spp` from `mpp`, and aim the eventual `mret`
/// at the supervisor's trap vector (`stvec`).
///
/// `secondary`, when present, is a guarded operation's recovered fault
/// `(cause, tval)`: the delivered `scause`/`stval` report the precise
/// secondary exception and its actually failing address instead of the
/// original trap's values; `sepc` still names the
/// original instruction.
///
/// # Contract
///
/// - Must be called while handling a trap, before any `mepc` advance: `sepc`
///   is read from the `mepc` CSR.
/// - Refuses traps originating from M-mode: forwarding the firmware's own
///   fault into the supervisor would be meaningless. Route those to failure
///   reporting instead.
pub fn redirect_trap(secondary: Option<(usize, usize)>) -> Result<(), Error> {
    if mstatus::read().mpp() == mstatus::MPP::Machine {
        return Err(Error::MachineOrigin);
    }
    let (cause, tval) = secondary.unwrap_or_else(|| (mcause::read().bits(), mtval::read()));
    // SAFETY: M-mode trap handling may write the S-mode trap CSRs and mepc;
    // the values programmed mirror what hardware would have written had the
    // trap been delegated to the supervisor.
    unsafe {
        sepc::write(mepc::read());
        scause::write(scause::Scause::from_bits(cause));
        stval::write(tval);
        // Mirror the hardware trap-entry semantics for the receiving mode:
        // spie ← sie and sie ← 0, so the supervisor's handler srets back
        // with exactly the interrupt-enable state it had before the trap.
        let sstatus_bits = sstatus::read().bits();
        let sstatus_bits =
            (sstatus_bits & !(SSTATUS_SIE | SSTATUS_SPIE)) | ((sstatus_bits & SSTATUS_SIE) << 4);
        sstatus::write(sstatus::Sstatus::from_bits(sstatus_bits));
        if mstatus::read().mpp() == mstatus::MPP::Supervisor {
            sstatus::set_spp(sstatus::SPP::Supervisor);
        } else {
            sstatus::set_spp(sstatus::SPP::User);
        }
        mstatus::set_mpp(mstatus::MPP::Supervisor);
        mepc::write(stvec::read().address());
    }
    Ok(())
}
