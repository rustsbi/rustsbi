//! CLINT timer and IPI services for a firmware-selected register layout.

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::Range;
use core::sync::atomic::{AtomicPtr, Ordering};

use crate::boot::BootInfo;
use crate::hart::{HartAdmission, IpiDevice, IpiError, Notification};
use crate::timer::Operations as TimerOperations;
use crate::{HartControl, Interrupts, IoMem, Ipi, RemoteFence, Timer, io_fence};

const MSIP: usize = 0;
const MTIMECMP: usize = 0x4000;
const MTIME: usize = 0xbff8;

/// The selected CLINT time source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeSource {
    /// Read the standard `mtime` MMIO register.
    MemoryMapped,
    /// Read the architectural `time` CSR used by T-Head-style CLINTs.
    Counter,
}

/// Complete CLINT facts decoded by firmware from its boot-local device tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Layout {
    range: Range<usize>,
    harts: Vec<usize>,
    time: TimeSource,
}

impl Layout {
    /// Creates a CLINT layout only when every selected hart register fits.
    pub fn new(range: Range<usize>, harts: Vec<usize>, time: TimeSource) -> Option<Self> {
        if range.start >= range.end
            || !range.start.is_multiple_of(8)
            || harts.is_empty()
            || harts
                .iter()
                .enumerate()
                .any(|(index, hart)| harts[..index].contains(hart))
        {
            return None;
        }
        for hart in &harts {
            let msip = range
                .start
                .checked_add(MSIP.checked_add(hart.checked_mul(4)?)?)?;
            let compare = range
                .start
                .checked_add(MTIMECMP.checked_add(hart.checked_mul(8)?)?)?;
            if msip.checked_add(4)? > range.end || compare.checked_add(8)? > range.end {
                return None;
            }
        }
        if time == TimeSource::MemoryMapped && range.start.checked_add(MTIME + 8)? > range.end {
            return None;
        }
        Some(Self { range, harts, time })
    }
}

/// Claims and installs one selected CLINT without reparsing any device tree.
pub fn install(boot: &mut BootInfo, layout: Layout) -> Option<Interrupts> {
    let registers = IoMem::acquire(boot, layout.range)?;
    let harts = layout.harts;
    let clint = Box::leak(Box::new(Clint {
        registers,
        time: layout.time,
        harts: harts.clone(),
    }));
    let pointer = clint as *mut Clint;
    let device: Arc<dyn IpiDevice> = Arc::new(ClintIpi(clint));
    let wake_by_ipi = alloc::vec![true; harts.len()];
    let runtime = HartAdmission::new(device, &harts, boot.init_hart_id(), &wake_by_ipi).ok()?;
    INSTALLED
        .compare_exchange(
            core::ptr::null_mut(),
            pointer,
            Ordering::AcqRel,
            Ordering::Relaxed,
        )
        .ok()?;
    boot.install_runtime(runtime.clone(), &TIMER)
        .then_some(())?;
    Some(Interrupts {
        timer: Timer::new(&TIMER),
        ipi: Ipi::new(runtime.clone()),
        remote_fence: RemoteFence::new(runtime.clone()),
        harts: HartControl::new(runtime),
    })
}

struct Clint {
    registers: IoMem,
    time: TimeSource,
    harts: Vec<usize>,
}

static INSTALLED: AtomicPtr<Clint> = AtomicPtr::new(core::ptr::null_mut());

static TIMER: TimerOperations = TimerOperations {
    prepare_current_hart: prepare_timer,
    read_time,
    set_deadline,
    handle_interrupt: handle_timer,
};

impl Clint {
    fn contains_hart(&self, hart: usize) -> bool {
        self.harts.contains(&hart)
    }

    fn msip(&self, hart: usize) -> Option<usize> {
        MSIP.checked_add(hart.checked_mul(4)?)
    }

    fn mtimecmp(&self, hart: usize) -> Option<usize> {
        MTIMECMP.checked_add(hart.checked_mul(8)?)
    }

    fn read_mtime(&self) -> u64 {
        loop {
            let high_before = self.registers.read_once::<u32>(MTIME + 4).unwrap_or(0);
            let low = self.registers.read_once::<u32>(MTIME).unwrap_or(0);
            let high_after = self.registers.read_once::<u32>(MTIME + 4).unwrap_or(0);
            if high_before == high_after {
                return (u64::from(high_after) << 32) | u64::from(low);
            }
        }
    }
}

fn installed() -> Option<&'static Clint> {
    let pointer = INSTALLED.load(Ordering::Acquire);
    // SAFETY: publication stores one leaked immutable CLINT before Release.
    unsafe { pointer.as_ref() }
}

fn prepare_timer() -> Result<(), crate::TimerError> {
    let clint = installed().ok_or(crate::TimerError::Unavailable)?;
    clint
        .contains_hart(current_hart_id())
        .then_some(())
        .ok_or(crate::TimerError::InvalidHart)
}

fn read_time() -> u64 {
    match installed().map(|clint| clint.time) {
        Some(TimeSource::MemoryMapped) => installed().map_or(0, Clint::read_mtime),
        Some(TimeSource::Counter) => read_time_csr(),
        None => 0,
    }
}

fn set_deadline(deadline: u64) {
    let hart = current_hart_id();
    let Some(clint) = installed().filter(|clint| clint.contains_hart(hart)) else {
        return;
    };
    let Some(offset) = clint.mtimecmp(hart) else {
        return;
    };
    let _ = clint.registers.write_once(offset, u32::MAX);
    let _ = clint
        .registers
        .write_once(offset + 4, (deadline >> 32) as u32);
    let _ = clint.registers.write_once(offset, deadline as u32);
    enable_machine_timer();
}

fn handle_timer() -> bool {
    manifest_supervisor_timer();
    true
}

struct ClintIpi(&'static Clint);

impl IpiDevice for ClintIpi {
    fn prepare_current_hart(&self) -> Result<(), IpiError> {
        let hart = current_hart_id();
        if !self.0.contains_hart(hart) {
            return Err(IpiError::InvalidHart);
        }
        self.claim(hart);
        enable_machine_software_interrupt();
        Ok(())
    }

    fn notify(&self, hart: usize) {
        let Some(offset) = self
            .0
            .contains_hart(hart)
            .then(|| self.0.msip(hart))
            .flatten()
        else {
            return;
        };
        io_fence();
        let _ = self.0.registers.write_once(offset, 1u32);
        io_fence();
    }

    fn claim(&self, hart: usize) {
        let Some(offset) = self
            .0
            .contains_hart(hart)
            .then(|| self.0.msip(hart))
            .flatten()
        else {
            return;
        };
        let _ = self.0.registers.write_once(offset, 0u32);
        io_fence();
    }

    fn notification(&self) -> Notification {
        Notification::Software
    }
}

fn enable_machine_timer() {
    const MTIE: usize = 1 << 7;
    const STIP: usize = 1 << 5;
    // SAFETY: clear a stale supervisor timer manifestation before MTIE.
    unsafe {
        core::arch::asm!(
            "csrc mip, {stip}",
            "csrs mie, {mtie}",
            stip = in(reg) STIP,
            mtie = in(reg) MTIE,
            options(nostack),
        )
    }
}

fn enable_machine_software_interrupt() {
    const MSIE: usize = 1 << 3;
    // SAFETY: the bound CLINT source was cleared before this local enable.
    unsafe { core::arch::asm!("csrs mie, {msie}", msie = in(reg) MSIE, options(nostack)) }
}

fn manifest_supervisor_timer() {
    const MTIE: usize = 1 << 7;
    const STIP: usize = 1 << 5;
    // SAFETY: mask MTIE before making the supervisor timer pending.
    unsafe {
        core::arch::asm!(
            "csrc mie, {mtie}",
            "csrs mip, {stip}",
            mtie = in(reg) MTIE,
            stip = in(reg) STIP,
            options(nostack),
        )
    }
}

fn current_hart_id() -> usize {
    let hart;
    // SAFETY: mhartid is a mandatory read-only machine CSR.
    unsafe {
        core::arch::asm!("csrr {hart}, mhartid", hart = out(reg) hart, options(nomem, nostack))
    };
    hart
}

fn read_time_csr() -> u64 {
    #[cfg(target_pointer_width = "64")]
    {
        let time;
        // SAFETY: time is a read-only architectural CSR.
        unsafe { core::arch::asm!("rdtime {time}", time = out(reg) time, options(nomem, nostack)) };
        time
    }
    #[cfg(target_pointer_width = "32")]
    loop {
        let high_before: u32;
        let low: u32;
        let high_after: u32;
        // SAFETY: high-low-high is the architectural stable RV32 read.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_hart_registers_must_fit_the_owned_window() {
        assert!(
            Layout::new(
                0x200_0000..0x201_0000,
                alloc::vec![0, 8],
                TimeSource::MemoryMapped
            )
            .is_some()
        );
        assert!(
            Layout::new(
                0x200_0000..0x200_4040,
                alloc::vec![8],
                TimeSource::MemoryMapped
            )
            .is_none()
        );
    }

    #[test]
    fn counter_time_does_not_require_an_mtime_mmio_register() {
        assert!(Layout::new(0x1000..0x6000, alloc::vec![0], TimeSource::Counter).is_some());
        assert!(Layout::new(0x1000..0x6000, alloc::vec![0], TimeSource::MemoryMapped).is_none());
    }
}
