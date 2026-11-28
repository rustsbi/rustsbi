//! IMSIC definitions and machine interrupt-file access.
//!
//! The [IMSIC specification](https://docs.riscv.org/reference/aia/v1.0/IMSIC.html)
//! defines the registers accessed through the `miselect` and `mireg` window.

use core::arch::asm;

use crate::{Read, Write};
pub use riscv_csr_spec::standard::aia::imsic::*;
use riscv_csr_spec::standard::isa::privileged::machine::{Mireg, Miselect};

/// Writes registers in the current hart's machine interrupt file, in order.
///
/// Returns `Some(())` after the last write or `None` at the first selector
/// invalid for the current XLEN. Earlier writes remain in effect.
/// Both returns restore the original `miselect`. A trap or an iterator panic
/// can interrupt the sequence before restoration.
///
/// # Safety
///
/// 1. Execute in M-mode with the machine interrupt file and selected registers
///    accessible. Supply values permitted by those registers.
/// 2. Exclude competing uses of this hart's selector and data window, including
///    from interrupt handlers, until the previous selector has been restored.
#[inline(always)]
pub unsafe fn write_machine_registers(
    writes: impl IntoIterator<Item = (ImsicRegister, usize)>,
) -> Option<()> {
    let previous = Miselect::read();
    let result = writes.into_iter().try_for_each(|(register, value)| {
        let selector = register.selector()?;
        // SAFETY: The caller owns this window and supplies an accessible register and value.
        unsafe {
            Miselect::write(selector);
            asm!("csrw {}, {}", const Mireg::NUMBER, in(reg) value, options(nostack));
        }
        Some(())
    });
    // SAFETY: The caller retains exclusive access through restoration.
    unsafe { Miselect::write(previous) };
    result
}
