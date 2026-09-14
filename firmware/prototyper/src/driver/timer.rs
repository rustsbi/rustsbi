//! Platform timer devices shared by boot, Runtime, and SBI TIME.

use crate::riscv::csr::{mie, mip, stimecmp};
use alloc::boxed::Box;
use runtime::hart::HartId;
use spin::{Mutex, Once};

/// Low-level timer backend used by the SBI timer device.
pub(crate) trait TimerBackend: Send {
    /// Programs the timer comparison value for `hart_id`.
    fn set_timer(&self, hart_id: usize, value: u64);

    /// Reads a device-provided time source for Runtime's `rdtime` fallback.
    ///
    /// Runtime first reads the architecture `time` CSR. A backend returns
    /// `Some` only when it also owns a readable counter, such as a CLINT
    /// `mtime`; a CSR-only backend returns `None`.
    fn read_time(&self) -> Option<u64> {
        None
    }
}

/// Timer implementation using the Sstc `stimecmp` CSR.
pub(super) struct SstcTimer;

impl TimerBackend for SstcTimer {
    #[inline(always)]
    fn set_timer(&self, hart_id: usize, value: u64) {
        if hart_id
            == HartId::current()
                .expect("BUG: current hart exceeds Runtime capacity")
                .as_usize()
        {
            stimecmp::set(value);
            if value == u64::MAX {
                mip::clear_stimer();
                mie::clear_mtimer();
            }
        }
    }
}

/// Synchronized access to the selected platform timer.
pub(crate) struct TimerDevice {
    backend: Mutex<Box<dyn TimerBackend>>,
}

static DEVICE: Once<TimerDevice> = Once::new();

impl TimerDevice {
    fn new(backend: Box<dyn TimerBackend>) -> Self {
        Self {
            backend: Mutex::new(backend),
        }
    }

    #[inline]
    pub(crate) fn set_timer(&self, hart_id: usize, value: u64) {
        self.backend.lock().set_timer(hart_id, value);
    }

    #[inline]
    fn read_time(&self) -> Option<u64> {
        self.backend.lock().read_time()
    }
}

impl runtime::timer::TimerDevice for TimerDevice {
    fn clear_current(&self) {
        let hart_id = HartId::current()
            .expect("BUG: current hart exceeds Runtime capacity")
            .as_usize();
        self.set_timer(hart_id, u64::MAX);
    }

    fn read_time(&self) -> Option<u64> {
        TimerDevice::read_time(self)
    }
}

/// Publishes the platform timer device.
pub(crate) fn init(backend: Box<dyn TimerBackend>) -> &'static TimerDevice {
    DEVICE.call_once(|| TimerDevice::new(backend))
}

/// Cancels a pending timer during hart boot.
pub(crate) fn clear_current() {
    if let Some(device) = DEVICE.get() {
        runtime::timer::TimerDevice::clear_current(device);
    }
}
