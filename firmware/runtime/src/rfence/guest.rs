//! Guest-context selection for complete remote HFENCE.VVMA operations.

use crate::csr::{Hgatp, Misa, Readable, Vsatp, Writable};
use crate::instructions::fence;
use crate::rfence::FenceError;

/// A guest context captured from the requesting hart's hardware.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct GuestContext {
    hgatp: usize,
}

impl GuestContext {
    pub(super) fn capture() -> Result<Self, FenceError> {
        if !Misa::read()
            .expect("misa is required in M-mode")
            .has_extension('H')
        {
            return Err(FenceError::HypervisorUnavailable);
        }
        Hgatp::read()
            .map(|hgatp| Self { hgatp })
            .map_err(FenceError::Access)
    }

    /// Executes a VVMA fence under the requesting hart's VMID and restores
    /// the executing hart's complete guest context before returning.
    pub(super) fn execute(self, invalidate_fn: impl FnOnce()) -> Result<(), FenceError> {
        riscv::interrupt::machine::free(|| {
            let old_hgatp = Hgatp::read().map_err(FenceError::Access)?;
            if old_hgatp == self.hgatp {
                invalidate_fn();
                return Ok(());
            }
            let old_vsatp = Vsatp::read().map_err(FenceError::Access)?;
            // Interrupts stay disabled for the entire operation. M-mode has
            // V=0 and performs no guest memory access here. Clear vsatp before
            // selecting another VMID, so speculative VS translations cannot
            // combine the old vsatp with the requesting guest's VMID.
            // Borrow a complete hardware-read hgatp rather than only its VMID:
            // MODE=Bare with nonzero remaining fields is unspecified.
            // https://docs.riscv.org/reference/isa/_attachments/riscv-privileged.pdf
            let result = (|| {
                write_checked::<Vsatp>(0)?;
                write_checked::<Hgatp>(self.hgatp)?;
                // A change of hgatp.MODE for a VMID requires GVMA ordering.
                // This extra invalidation is permitted by RFENCE semantics.
                fence::hfence_gvma_all();
                invalidate_fn();
                Ok(())
            })();

            // Restore hgatp before re-enabling the previous VS translation.
            // No error may expose the borrowed context to the interrupted
            // guest. A failed restoration cannot return to normal execution.
            if write_checked::<Hgatp>(old_hgatp).is_err() {
                fail_stop();
            }
            fence::hfence_gvma_all();
            if write_checked::<Vsatp>(old_vsatp).is_err() {
                fail_stop();
            }
            result
        })
    }
}

fn write_checked<R: Readable<Value = usize> + Writable>(value: usize) -> Result<(), FenceError> {
    R::write(value).map_err(FenceError::Access)?;
    let actual = R::read().map_err(FenceError::Access)?;
    if actual != value {
        return Err(FenceError::GuestContextUnavailable);
    }
    Ok(())
}

#[allow(unsafe_code)]
fn fail_stop() -> ! {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    // Returning would expose a guest context that was not completely restored.
    // SAFETY:
    // 1. Restoration runs in M-mode with machine interrupts disabled.
    // 2. The terminal vector never returns or needs this call chain's stack.
    unsafe {
        core::arch::asm!(
            "j {fail}",
            fail = sym crate::boot::fail_stop,
            options(noreturn)
        )
    }
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    panic!("failed to restore the remote-fence guest context");
}
