//! Runtime readiness and linker-table lookup for the [CSR recovery ABI](csr::recovery).

use csr::{
    Read,
    recovery::FaultSite,
    standard::isa::{
        privileged::machine::{Mscratch, Mstatus, Mtvec},
        values::vector::VectorMode,
    },
};

/// Checks the current hart's trap vector and scratch state before a recoverable access.
///
/// Requires a direct Runtime vector, `MPRV = 0` and `mscratch = 0`.
/// The [recovery ABI](csr::recovery) also requires a valid Runtime stack,
/// deliverable machine traps and preservation of any live trap-return state.
pub(crate) fn is_ready() -> bool {
    let vector = Mtvec::read();
    vector.get(Mtvec::MODE) == Ok(VectorMode::Direct)
        && crate::trap::entry::is_runtime_entry(vector.get(Mtvec::BASE).address().bits())
        && !Mstatus::read().get(Mstatus::MPRV)
        && Mscratch::read() == 0
}

impl From<csr::AccessError> for crate::Error {
    fn from(error: csr::AccessError) -> Self {
        match error {
            csr::AccessError::IllegalInstruction => Self::IllegalInstruction,
        }
    }
}

/// Returns the registered continuation for a fault PC and accepted cause.
// Dispatch restricts recovery to machine-origin exceptions.
pub(crate) fn find_fixup(pc: usize, cause: usize) -> Option<usize> {
    unsafe extern "C" {
        static __trap_fixup_start: FaultSite;
        static __trap_fixup_end: FaultSite;
    }
    let start = &raw const __trap_fixup_start;
    let bytes = (&raw const __trap_fixup_end as usize) - start as usize;
    // SAFETY: The linker collects aligned, initialized `FaultSite` records
    // into one immutable section between these symbols.
    let sites = unsafe { core::slice::from_raw_parts(start, bytes / size_of::<FaultSite>()) };
    sites.iter().find_map(|site| site.fixup(pc, cause))
}
