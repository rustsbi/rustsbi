//! Hart-local Sstc timer mechanism used by AIA installation.

use super::{Operations, Timer, TimerError};
use crate::trap::probe::{ExpectedResult, probe_csr, swap_csr};

const MCOUNTEREN: u16 = 0x306;
#[cfg(target_pointer_width = "64")]
const MENVCFG: u16 = 0x30a;
#[cfg(target_pointer_width = "32")]
const MENVCFGH: u16 = 0x31a;
const COUNTEREN_TIME: usize = 1 << 1;

static SSTC_TIMER: Operations = Operations {
    prepare_current_hart: prepare_sstc,
    read_time,
    set_deadline: write_stimecmp,
    handle_interrupt: no_machine_interrupt,
};

pub(crate) fn install(harts: &[usize]) -> Result<Timer, TimerError> {
    if harts.is_empty() {
        return Err(TimerError::InvalidHart);
    }
    Ok(Timer::new(&SSTC_TIMER))
}

fn no_machine_interrupt() -> bool {
    false
}

fn prepare_sstc() -> Result<(), TimerError> {
    if crate::hart::resolve(current_hart_id()).is_none() {
        return Err(TimerError::InvalidHart);
    }
    // SAFETY: this mechanism owns these fixed CSR and capability bits.
    unsafe {
        set_csr_bits::<MCOUNTEREN>(COUNTEREN_TIME)?;
        #[cfg(target_pointer_width = "64")]
        set_csr_bits::<MENVCFG>(1usize << 63)?;
        #[cfg(target_pointer_width = "32")]
        set_csr_bits::<MENVCFGH>(1usize << 31)?;
    }
    Ok(())
}

unsafe fn set_csr_bits<const CSR: u16>(bits: usize) -> Result<(), TimerError> {
    // SAFETY: callers select only the fixed CSR constants above.
    let original = match unsafe { probe_csr::<CSR>() } {
        ExpectedResult::Value(value) => value,
        _ => return Err(TimerError::Unavailable),
    };
    // SAFETY: same fixed CSR and WARL bit set.
    match unsafe { swap_csr::<CSR>(original | bits) } {
        ExpectedResult::Value(value) if value == original => {}
        _ => return Err(TimerError::Unavailable),
    }
    // SAFETY: readback of that same fixed CSR.
    if matches!(unsafe { probe_csr::<CSR>() }, ExpectedResult::Value(value) if value & bits == bits)
    {
        return Ok(());
    }
    // SAFETY: restore the original value captured above.
    let _ = unsafe { swap_csr::<CSR>(original) };
    Err(TimerError::Unavailable)
}

fn current_hart_id() -> usize {
    let value;
    // SAFETY: mhartid is a mandatory read-only machine CSR.
    unsafe {
        core::arch::asm!("csrr {value}, mhartid", value = out(reg) value, options(nomem, nostack))
    };
    value
}

fn read_time() -> u64 {
    #[cfg(target_pointer_width = "64")]
    {
        let value;
        // SAFETY: time is read-only and preparation enabled its visibility.
        unsafe {
            core::arch::asm!("rdtime {value}", value = out(reg) value, options(nomem, nostack))
        };
        value
    }
    #[cfg(target_pointer_width = "32")]
    loop {
        let high_before: u32;
        let low: u32;
        let high_after: u32;
        // SAFETY: high-low-high is the stable RV32 counter sequence.
        unsafe {
            core::arch::asm!(
                "rdtimeh {high_before}",
                "rdtime {low}",
                "rdtimeh {high_after}",
                high_before = out(reg) high_before,
                low = out(reg) low,
                high_after = out(reg) high_after,
                options(nomem, nostack),
            )
        };
        if high_before == high_after {
            break (u64::from(high_after) << 32) | u64::from(low);
        }
    }
}

fn write_stimecmp(deadline: u64) {
    #[cfg(target_pointer_width = "64")]
    // SAFETY: preparation established that stimecmp exists.
    unsafe {
        core::arch::asm!("csrw 0x14d, {value}", value = in(reg) deadline, options(nomem, nostack))
    }
    #[cfg(target_pointer_width = "32")]
    // SAFETY: max-low/high/final-low prevents a transient early deadline.
    unsafe {
        core::arch::asm!(
            "csrw 0x14d, {maximum}",
            "csrw 0x15d, {high}",
            "csrw 0x14d, {low}",
            maximum = in(reg) u32::MAX,
            high = in(reg) (deadline >> 32) as u32,
            low = in(reg) deadline as u32,
            options(nomem, nostack),
        )
    }
}
