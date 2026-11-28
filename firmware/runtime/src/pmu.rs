//! Hardware performance counters, event selection and counting control.

use csr::{
    Clear, Read, Set, Write,
    standard::isa::privileged::machine::{
        self, InhibitedCounters, Mcountinhibit, Mcycle, Minstret,
    },
};
pub use machine::{HardwareCounter, HpmCounter};

use crate::{Error, Result};

/// Programs the event selector for a hardware performance counter.
///
/// On RV32, a selector with no accessible high word accepts only 32-bit values.
/// Updating both words requires an inhibit bit that reads back as set.
/// A failed low-word write can leave the new high word in effect.
/// Reprogram the selector before resuming that counter.
/// Success means the CSR writes completed. Hardware may coerce selector fields.
///
/// # Safety
///
/// Execute in M-mode while exclusively owning this counter and event selector.
/// Stop accumulation before an RV32 multiword update and coordinate its readers.
pub unsafe fn write_event(counter: HpmCounter, value: u64) -> Result<()> {
    if !crate::recovery::is_ready() {
        return Err(Error::NotReady);
    }
    crate::irq::with_local_disabled(|_| {
        #[cfg(target_pointer_width = "32")]
        {
            // SAFETY: Runtime recovery is ready with local interrupts excluded;
            // the caller owns this counter's event selector.
            let has_high = unsafe { machine::try_read_mhpmeventh(counter) }.is_ok();
            if has_high {
                // SAFETY: The same recovery window covers the optional inhibit CSR.
                let inhibited =
                    unsafe { Mcountinhibit::try_read() }.map_err(|_| Error::NotSupported)?;
                if !inhibited.contains(HardwareCounter::Hpm(counter)) {
                    return Err(Error::NotSupported);
                }
                // SAFETY: The probe established the high word; the caller keeps
                // accumulation stopped throughout the multiword update.
                unsafe { machine::try_write_mhpmeventh(counter, (value >> 32) as u32)? };
            } else if value >> 32 != 0 {
                return Err(Error::NotSupported);
            }
        }
        // SAFETY: The same Runtime recovery and ownership context covers the
        // native-word write; RV32 high bits were handled without truncation above.
        unsafe { machine::try_write_mhpmevent(counter, value as usize)? };
        Ok(())
    })
}

/// Reports whether `counter` is currently inhibited in `mcountinhibit`.
pub fn is_inhibited(counter: HardwareCounter) -> bool {
    Mcountinhibit::read().contains(counter)
}

/// Sets the implemented HPM inhibit bits (3 through 31), preserving `CY` and `IR`.
///
/// # Safety
///
/// Execute in M-mode on a hart supporting `mcountinhibit` while owning PMU initialization.
pub unsafe fn inhibit_programmable() {
    // SAFETY: The caller owns the HPM counters. CSRRS leaves a shared CY bit untouched.
    unsafe { Mcountinhibit::write(Set(InhibitedCounters::programmable())) };
}

/// Sets or clears the inhibit bit for `counter` in `mcountinhibit`.
/// Returns [`Error::NotSupported`] if the bit does not accept the requested state.
///
/// # Safety
///
/// Execute in M-mode on a hart supporting `mcountinhibit` and coordinate shared
/// counter state (including `CY` across harts if shared).
pub unsafe fn set_inhibited(counter: HardwareCounter, inhibited: bool) -> Result<()> {
    // SAFETY: The caller coordinates all harts sharing this counter. The
    // atomic set or clear touches only its inhibit bit.
    unsafe {
        if inhibited {
            Mcountinhibit::write(Set(counter));
        } else {
            Mcountinhibit::write(Clear(counter));
        }
    }
    if is_inhibited(counter) != inhibited {
        return Err(Error::NotSupported);
    }
    // QEMU 9.0 marks a counter stopped when it is first read with its inhibit
    // bit set. Consume that transition before the supervisor reads cycle or instret.
    // https://github.com/qemu/qemu/blob/v9.0.0/target/riscv/csr.c#L921-L944
    match counter {
        HardwareCounter::Cycle => {
            let _ = Mcycle::read();
        }
        HardwareCounter::InstructionsRetired => {
            let _ = Minstret::read();
        }
        HardwareCounter::Hpm(_) => {}
    }
    Ok(())
}

/// Writes a 64-bit hardware performance counter.
///
/// # Safety
///
/// Execute in M-mode on a hart implementing the selected counter. Coordinate
/// writers across all harts that share it. On RV32, inhibit accumulation for an
/// exact update. Readers can observe an intermediate value unless the caller
/// excludes them across both writes.
pub unsafe fn write_counter(counter: HardwareCounter, value: u64) {
    // SAFETY: The caller establishes hardware support and coordinates writers,
    // including other harts for a shared counter.
    unsafe {
        match counter {
            HardwareCounter::Cycle => Mcycle::write(value),
            HardwareCounter::InstructionsRetired => Minstret::write(value),
            HardwareCounter::Hpm(counter) => machine::write_mhpmcounter(counter, value),
        }
    }
}
