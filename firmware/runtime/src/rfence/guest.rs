//! VMID selection used by remote HFENCE.VVMA requests.

use crate::{Error, Result, irq::LocalIrqDisabled};
use csr::{
    hypervisor::{self, GuestAddressSpace},
    virtual_supervisor,
};

// Capture and execution check HSXLEN == VSXLEN == MXLEN before using
// these native-width layouts. Runtime never switches SXL or VSXL here.
const MODE_SHIFT: u32 = if usize::BITS == 32 { 31 } else { 60 };
fn hgatp_vmid(hgatp: usize) -> usize {
    GuestAddressSpace::from_bits(hgatp).vmid()
}

fn replace_vmid(hgatp: usize, vmid: usize) -> usize {
    GuestAddressSpace::from_bits(hgatp).with_vmid(vmid).bits()
}

/// Only the calling hart's VMID is needed by SBI HFENCE.VVMA. Its translation
/// mode and root need not be supported or installed on the target hart.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct GuestContext {
    vmid: usize,
}

impl GuestContext {
    pub(super) fn capture() -> Result<Self> {
        if !crate::hart::has_hypervisor() {
            return Err(Error::NotSupported);
        }
        super::xlen::check(super::FenceOperation::HFenceVvma)?;
        let hgatp = crate::csr::csrr!(hypervisor::GuestAddressTranslation)?;
        Ok(Self {
            vmid: hgatp_vmid(hgatp),
        })
    }

    /// Invalidates under this VMID, restoring the target's complete guest state
    /// on both Ok and Err. Restoration failure stops the hart.
    ///
    /// # Safety
    ///
    /// The caller must remain in M-mode on the checked executor's hart, with a
    /// valid Runtime stack/trap entry, mscratch=0 and MPRV=0 throughout. `irq`
    /// must exclude local machine interrupts for the entire call. The caller
    /// must check HSXLEN == VSXLEN == MXLEN on this hart before calling, and
    /// keep those widths unchanged until restoration. No guest or guest-memory
    /// access may run while the context is borrowed. `invalidate`
    /// must only perform Runtime's local fence operations, without unwinding,
    /// reentering, changing guest CSRs, or enabling interrupts.
    pub(super) unsafe fn with(
        self,
        irq: &LocalIrqDisabled,
        invalidate: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        // SAFETY: The caller supplies the M-mode/recovery and exclusion scope
        // required by with_vmid. Hardware uses only this scope's hart-local
        // CSR instructions; invalidate inherits the same restrictions.
        unsafe { with_vmid(self.vmid, &mut Hardware(irq), invalidate) }
    }
}

/// Selects a VMID without borrowing the caller's translation mode or root.
///
/// # Safety
///
/// For a hardware backend, the caller must maintain GuestContext::with's
/// M-mode, trap, stack and exclusion preconditions until this function returns
/// or stops the hart. `vmid` must fit the architectural VMID field. The callback
/// must not access guest memory, mutate guest CSRs, enable interrupts, reenter
/// or unwind. A simulated backend must have
/// no architectural effects; its callbacks may unwind without exposing machine
/// state. Registers may be partially written on Err; this function restores
/// both saved CSRs before returning, or terminates on restoration failure.
unsafe fn with_vmid(
    vmid: usize,
    registers: &mut impl GuestRegisters,
    invalidate: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let old_hgatp = registers.hgatp()?;
    if hgatp_vmid(old_hgatp) == vmid {
        return invalidate();
    }
    let old_vsatp = registers.vsatp()?;
    // SAFETY: The caller excludes guest accesses and local interrupts. hgatp
    // is still unchanged; a failed zeroing only needs the saved vsatp restored.
    if let Err(error) = unsafe { registers.set_vsatp(0) } {
        // SAFETY: hgatp has not been touched. Restore the saved vsatp under the
        // same exclusion; a failed restoration cannot return to the caller.
        if unsafe { registers.set_vsatp(old_vsatp) }.is_err() {
            // SAFETY: This is the caller's still-excluded execution scope;
            // restoration failed, so the hart must never resume guest accesses.
            unsafe { fail_stop() };
        }
        return Err(error);
    }
    // SAFETY: The caller excludes guest accesses, interrupts and unwinding.
    // vsatp=0 was verified before any hgatp change (ISA section 5.5.3); select_vmid
    // does not access guest memory. Both saved values remain local, and all
    // results from this block pass through the restoration below.
    let result = unsafe {
        (|| {
            select_vmid(registers, old_hgatp, vmid)?;
            registers.fence_gvma()?;
            invalidate()
        })()
    };

    // SAFETY: Exclusion still holds. No intervening operation changes vsatp
    // after its zeroing (even failed writes may have effects). Restoring hgatp
    // uses the exact saved value; the readback and GVMA must succeed before
    // restoring vsatp. Every failed restoration is terminal, so no caller can
    // resume guest execution under a partially restored context.
    let restored = unsafe {
        registers.set_hgatp(old_hgatp) == Ok(old_hgatp)
            && registers.fence_gvma().is_ok()
            && registers.set_vsatp(old_vsatp).is_ok()
    };
    if !restored {
        // SAFETY: The hardware execution/exclusion scope is still active.
        // At least one saved CSR or required fence could not be restored.
        unsafe { fail_stop() };
    }
    result
}

/// # Safety
///
/// Requires with_vmid's hardware exclusion/recovery conditions, vsatp=0, and
/// no guest accesses until the caller has restored the original CSRs or stopped
/// the hart. `old_hgatp` must be this backend's saved entry value, and `vmid`
/// must fit its architectural field. Both Ok and Err may leave hgatp changed.
/// Ok establishes the exact requested VMID in a non-Bare mode; it does not
/// authorize guest execution.
unsafe fn select_vmid(
    registers: &mut impl GuestRegisters,
    old_hgatp: usize,
    vmid: usize,
) -> Result<()> {
    if old_hgatp >> MODE_SHIFT != 0 {
        // Preserve the executing hart's own supported MODE and root.
        let hgatp = replace_vmid(old_hgatp, vmid);
        // SAFETY: vsatp=0 and guest accesses are excluded by the caller; the
        // caller restores even partially applied writes. This changes only VMID.
        let observed = unsafe { registers.set_hgatp(hgatp)? };
        return (observed == hgatp).then_some(()).ok_or(Error::NotSupported);
    }

    // Bare requires all other hgatp fields to be zero. Try architecturally
    // defined paged modes with PPN=0 instead of writing Bare with a VMID.
    // No page walk uses that temporary root: vsatp=0 and no guest access runs.
    let modes: &[usize] = if usize::BITS == 32 { &[1] } else { &[8, 9, 10] };
    for mode in modes {
        let hgatp = replace_vmid(mode << MODE_SHIFT, vmid);
        // SAFETY: Same exclusion and restoration obligations as above. An
        // unsupported mode may normalize through WARL; do not require the
        // proposed MODE/PPN to survive, only a non-Bare mode with this VMID.
        let observed = unsafe { registers.set_hgatp(hgatp)? };
        if observed >> MODE_SHIFT != 0 && hgatp_vmid(observed) == vmid {
            return Ok(());
        }
    }
    Err(Error::NotSupported)
}

/// Private backend for the VMID state machine.
///
/// # Safety
///
/// Implementers must access only one hart's hgatp/vsatp (or an isolated model
/// of them). Reads return the current value without changing it. set_hgatp
/// returns the actual readback, and successful set_vsatp verifies exact
/// readback; errors may leave either write partially applied. fence_gvma must
/// order all G-stage translations. Methods must not access guest memory,
/// reenter, unwind, enable interrupts, or change unrelated machine state.
unsafe trait GuestRegisters {
    fn hgatp(&mut self) -> Result<usize>;
    fn vsatp(&mut self) -> Result<usize>;

    /// Writes hgatp and returns its WARL readback.
    ///
    /// # Safety
    ///
    /// For hardware, the caller must provide with_vmid's M-mode/trap/exclusion
    /// environment, keep vsatp=0 and exclude guest accesses until the saved
    /// context and required translation ordering are restored or the hart
    /// stops. Even Err may change hgatp; readback is representability evidence,
    /// not validation that the temporary root is safe for guest accesses.
    unsafe fn set_hgatp(&mut self, value: usize) -> Result<usize>;

    /// Writes vsatp and checks exact readback.
    ///
    /// # Safety
    ///
    /// For hardware, the caller must maintain with_vmid's M-mode, trap, stack
    /// and interrupt exclusion conditions throughout this write and recovery.
    /// `value` must be zero while borrowing a VMID, or the exact saved vsatp
    /// when hgatp was never changed or after restoring hgatp and executing
    /// GVMA. No guest accesses may resume until both saved CSRs are restored;
    /// Err requires restoration or stopping.
    unsafe fn set_vsatp(&mut self, value: usize) -> Result<()>;
    fn fence_gvma(&mut self) -> Result<()>;
}

/// Constructed only inside GuestContext::with's checked execution scope.
///
/// # Safety-usable invariant
///
/// The caller's Runtime stack/trap, M-mode, mscratch=0 and MPRV=0 environment
/// remains installed while this value is used; its borrowed IRQ token excludes
/// local interrupts. No guest access runs during any method.
struct Hardware<'a>(&'a LocalIrqDisabled);

// SAFETY: Fixed CSR numbers select only hgatp/vsatp. Writes below perform
// immediate readback, and fence_gvma provides the complete G-stage fence.
// Hardware's invariant supplies the recoverable CSR environment; these methods
// contain no callbacks, guest accesses or changes to interrupt enable state.
unsafe impl GuestRegisters for Hardware<'_> {
    fn hgatp(&mut self) -> Result<usize> {
        crate::csr::csrr!(hypervisor::GuestAddressTranslation)
    }

    fn vsatp(&mut self) -> Result<usize> {
        crate::csr::csrr!(virtual_supervisor::AddressTranslation)
    }

    unsafe fn set_hgatp(&mut self, value: usize) -> Result<usize> {
        // SAFETY: Hardware's invariant establishes recovery on this hart. The
        // caller's set_hgatp contract excludes live guest/MPRV accesses and
        // retains responsibility for restoration after both Ok and Err.
        crate::csr::csrw!(hypervisor::GuestAddressTranslation, value)?;
        self.hgatp()
    }

    unsafe fn set_vsatp(&mut self, value: usize) -> Result<()> {
        // SAFETY: Hardware's invariant establishes recovery, and set_vsatp's
        // caller supplies zero or the saved value at the documented transition.
        // Readback does not discharge that caller's restoration obligations.
        crate::csr::csrw!(virtual_supervisor::AddressTranslation, value)?;
        (self.vsatp()? == value)
            .then_some(())
            .ok_or(Error::NotSupported)
    }

    fn fence_gvma(&mut self) -> Result<()> {
        super::instructions::hfence_gvma(self.0, None, None)
    }
}

/// # Safety
///
/// Hardware callers must still be in with_vmid's M-mode recovery/exclusion
/// scope. The saved context may be incomplete; this never returns. On a host
/// model, panicking terminates the simulated transition without machine effects.
#[cold]
unsafe fn fail_stop() -> ! {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    // SAFETY: A restoration failure reached this branch with interrupts still
    // excluded. boot::fail_stop is an M-mode assembly jump target that needs no
    // stack and never returns; no borrowed guest state can escape this call.
    unsafe {
        core::arch::asm!(
            "j {fail}",
            fail = sym crate::boot::fail_stop,
            options(noreturn)
        )
    }
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    panic!("failed to restore guest translation context")
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use alloc::{vec, vec::Vec};
    use std::panic::{AssertUnwindSafe, catch_unwind};

    const VMID_SHIFT: u32 = if usize::BITS == 32 { 22 } else { 44 };
    const VMID_MASK: usize = if usize::BITS == 32 { 0x7f } else { 0x3fff };

    #[derive(Debug, Eq, PartialEq)]
    enum Event {
        ReadHgatp,
        ReadVsatp,
        WriteHgatp(usize),
        WriteVsatp(usize),
        Gvma,
    }

    const PAGED_MODE: usize = if usize::BITS == 32 { 1 } else { 8 };
    const OLD_HGATP: usize = (PAGED_MODE << MODE_SHIFT) | (1 << VMID_SHIFT) | 0x400;
    const BORROWED_HGATP: usize = (PAGED_MODE << MODE_SHIFT) | (2 << VMID_SHIFT) | 0x400;

    struct Fake {
        hgatp: usize,
        vsatp: usize,
        fail_at: Option<usize>,
        fail_also_at: Option<usize>,
        events: Vec<Event>,
        supported_modes: &'static [usize],
        vmid_mask: usize,
    }

    impl Fake {
        fn new(fail_at: Option<usize>) -> Self {
            Self {
                hgatp: OLD_HGATP,
                vsatp: 10,
                fail_at,
                fail_also_at: None,
                events: Vec::new(),
                supported_modes: if usize::BITS == 32 { &[1] } else { &[8] },
                vmid_mask: if usize::BITS == 32 { 0x7f } else { 0x3fff },
            }
        }

        fn step(&mut self, event: Event) -> Result<()> {
            let index = self.events.len();
            self.events.push(event);
            if self.fail_at == Some(index) || self.fail_also_at == Some(index) {
                Err(Error::NotSupported)
            } else {
                Ok(())
            }
        }

        fn check_restored(&self) {
            assert_eq!((self.hgatp, self.vsatp), (OLD_HGATP, 10));
        }
    }

    // SAFETY: This backend only mutates its owned integer fields and event
    // vector. Reads are faithful; writes model WARL and may fail after mutation.
    // It never touches machine CSRs or guest memory and has no callbacks.
    unsafe impl GuestRegisters for Fake {
        fn hgatp(&mut self) -> Result<usize> {
            self.step(Event::ReadHgatp)?;
            Ok(self.hgatp)
        }

        fn vsatp(&mut self) -> Result<usize> {
            self.step(Event::ReadVsatp)?;
            Ok(self.vsatp)
        }

        unsafe fn set_hgatp(&mut self, value: usize) -> Result<usize> {
            let mode = value >> MODE_SHIFT;
            // Model an unsupported mode normalizing to Bare through WARL.
            self.hgatp = if self.supported_modes.contains(&mode) {
                replace_vmid(value, hgatp_vmid(value) & self.vmid_mask)
            } else {
                0
            };
            self.step(Event::WriteHgatp(value))?;
            Ok(self.hgatp)
        }

        unsafe fn set_vsatp(&mut self, value: usize) -> Result<()> {
            self.vsatp = value;
            self.step(Event::WriteVsatp(value))
        }

        fn fence_gvma(&mut self) -> Result<()> {
            self.step(Event::Gvma)
        }
    }

    fn run(
        vmid: usize,
        registers: &mut Fake,
        invalidate: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        // SAFETY: Fake has no architectural effects. with_vmid explicitly
        // permits model callbacks to unwind without exposing machine state.
        unsafe { with_vmid(vmid, registers, invalidate) }
    }

    #[test]
    fn only_the_callers_vmid_is_transferred_across_different_modes_and_roots() {
        let caller_mode = if usize::BITS == 32 { 1 } else { 9 }; // Sv48x4 on RV64
        let caller = (caller_mode << MODE_SHIFT) | (2 << VMID_SHIFT) | 0x800;
        let context = GuestContext {
            vmid: hgatp_vmid(caller),
        };
        let mut registers = Fake::new(None); // target: Sv39x4, a different PPN
        assert_eq!(run(context.vmid, &mut registers, || Ok(())), Ok(()));
        registers.check_restored();
        let written: Vec<_> = registers
            .events
            .iter()
            .filter_map(|event| {
                if let Event::WriteHgatp(value) = event {
                    Some(*value)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(written, vec![BORROWED_HGATP, OLD_HGATP]);
        assert_ne!(BORROWED_HGATP, caller);
    }

    #[test]
    fn bare_target_selects_a_supported_paged_mode_without_a_guest_root() {
        let mut registers = Fake::new(None);
        registers.hgatp = 0;
        // RV64 target deliberately supports only Sv48x4. The first Sv39x4
        // attempt normalizes to Bare; selection must continue to the next mode.
        registers.supported_modes = if usize::BITS == 32 { &[1] } else { &[9] };
        assert_eq!(run(2, &mut registers, || Ok(())), Ok(()));
        assert_eq!((registers.hgatp, registers.vsatp), (0, 10));
        let borrowed: Vec<_> = registers
            .events
            .iter()
            .filter_map(|event| {
                if let Event::WriteHgatp(value) = event {
                    Some(*value)
                } else {
                    None
                }
            })
            .collect();
        for value in borrowed.iter().copied().filter(|value| *value != 0) {
            assert_ne!(value >> MODE_SHIFT, 0, "never write Bare with a VMID");
            assert_eq!(value & ((1 << VMID_SHIFT) - 1), 0, "temporary PPN is zero");
        }
        assert_eq!(borrowed.len(), if usize::BITS == 32 { 2 } else { 3 });
    }

    #[test]
    fn unrepresentable_vmid_restores_without_invalidating_the_wrong_scope() {
        for initial in [0, OLD_HGATP] {
            let mut registers = Fake::new(None);
            registers.hgatp = initial;
            registers.vmid_mask = 1; // VMID 2 cannot be represented.
            assert_eq!(
                run(2, &mut registers, || panic!(
                    "wrong VMID must not be fenced"
                )),
                Err(Error::NotSupported)
            );
            assert_eq!((registers.hgatp, registers.vsatp), (initial, 10));
        }
    }

    #[test]
    fn bare_only_target_restores_after_all_mode_candidates_are_rejected() {
        let mut registers = Fake::new(None);
        registers.hgatp = 0;
        registers.supported_modes = &[];
        assert_eq!(
            run(2, &mut registers, || panic!("must not invalidate")),
            Err(Error::NotSupported)
        );
        assert_eq!((registers.hgatp, registers.vsatp), (0, 10));
    }

    #[test]
    fn failed_zeroing_restoration_is_terminal_before_any_hgatp_write() {
        let mut registers = Fake::new(Some(2));
        registers.fail_also_at = Some(3);
        let result = catch_unwind(AssertUnwindSafe(|| run(2, &mut registers, || Ok(()))));
        assert!(result.is_err());
        assert_eq!(registers.hgatp, OLD_HGATP);
        assert_eq!(
            registers.events,
            vec![
                Event::ReadHgatp,
                Event::ReadVsatp,
                Event::WriteVsatp(0),
                Event::WriteVsatp(10),
            ]
        );
    }

    #[test]
    fn context_switch_excludes_old_vs_translations_and_restores_in_order() {
        let mut registers = Fake::new(None);
        assert_eq!(run(2, &mut registers, || Ok(())), Ok(()));
        registers.check_restored();
        assert_eq!(
            registers.events,
            vec![
                Event::ReadHgatp,
                Event::ReadVsatp,
                Event::WriteVsatp(0),
                Event::WriteHgatp(BORROWED_HGATP),
                Event::Gvma,
                Event::WriteHgatp(OLD_HGATP),
                Event::Gvma,
                Event::WriteVsatp(10),
            ]
        );
    }

    #[test]
    fn matching_context_needs_no_guest_state_writes() {
        let mut registers = Fake::new(None);
        assert_eq!(
            run(1, &mut registers, || Err(Error::AccessDenied)),
            Err(Error::AccessDenied)
        );
        registers.check_restored();
        assert_eq!(registers.events, vec![Event::ReadHgatp]);
    }

    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    #[test]
    fn empty_vvma_bypasses_guest_csrs_and_vmid_selection() {
        use super::super::{AddressRange, FenceRequest, RequestOperation};

        // SAFETY: This host-only test has no machine interrupts or architectural
        // guest state. The token stays on this thread and within the test.
        let irq = unsafe { LocalIrqDisabled::assume_disabled() };
        for vmid in [0, 1, VMID_MASK] {
            let guest = GuestContext { vmid };
            for operation in [
                RequestOperation::HFenceVvma { guest },
                RequestOperation::HFenceVvmaAsid {
                    asid: usize::MAX,
                    guest,
                },
            ] {
                for start in [4096, usize::MAX & !4095] {
                    let request = FenceRequest {
                        operation,
                        range: AddressRange::new(start, 0).unwrap(),
                    };
                    for limit in [0, usize::MAX] {
                        // Host CSR access panics: success in the actual dispatcher
                        // proves this path needs no guest state or VMID support.
                        assert_eq!(request.execute_local(&irq, limit), Ok(()));
                    }
                }
                let nonempty = FenceRequest {
                    operation,
                    range: AddressRange::All,
                };
                assert!(
                    catch_unwind(AssertUnwindSafe(|| nonempty.execute_local(&irq, 0))).is_err(),
                    "whole-space requests must still enter the hardware path"
                );
            }
        }
    }

    #[test]
    fn failed_initial_reads_leave_context_untouched() {
        for step in [0, 1] {
            let mut registers = Fake::new(Some(step));
            assert_eq!(
                run(2, &mut registers, || panic!("must not invalidate")),
                Err(Error::NotSupported)
            );
            registers.check_restored();
            assert!(
                registers
                    .events
                    .iter()
                    .all(|event| matches!(event, Event::ReadHgatp | Event::ReadVsatp))
            );
        }
    }

    #[test]
    fn partially_applied_preparation_is_restored_before_error_return() {
        for step in [2, 3, 4] {
            let mut registers = Fake::new(Some(step));
            assert_eq!(
                run(2, &mut registers, || panic!("must not invalidate")),
                Err(Error::NotSupported)
            );
            registers.check_restored();
        }
    }

    #[test]
    fn invalidation_error_still_restores_guest_state() {
        let mut registers = Fake::new(None);
        assert_eq!(
            run(2, &mut registers, || Err(Error::NotSupported)),
            Err(Error::NotSupported)
        );
        registers.check_restored();
    }

    #[test]
    fn every_restoration_failure_is_terminal() {
        for step in [5, 6, 7] {
            let mut registers = Fake::new(Some(step));
            let result = catch_unwind(AssertUnwindSafe(|| run(2, &mut registers, || Ok(()))));
            assert!(
                result.is_err(),
                "restoration step {step} returned to its caller"
            );
        }
    }
}
