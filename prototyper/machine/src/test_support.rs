//! Post-initialization bridge into the dedicated machine-test framework.

use alloc::boxed::Box;
use core::mem::size_of;
use core::sync::atomic::{AtomicUsize, Ordering};

use machine_test::{RegistryError, Tests};
use spin::Once;

use crate::{BootInfo, Console, SbiCall, SbiHandler, SbiResponse, TrapEvent};

static CONSOLE: Once<Console> = Once::new();
static WARM_PARKED: AtomicUsize = AtomicUsize::new(0);

struct UnexpectedTrap;

impl SbiHandler for UnexpectedTrap {
    fn handle_ecall(&self, call: SbiCall) -> SbiResponse {
        if let Some(console) = CONSOLE.get() {
            let _ = console.try_write_fmt(format_args!(
                "@@RUSTSBI_MTEST type=UNEXPECTED_SBI_CALL call={call:?}\n"
            ));
        }
        crate::trap::abort()
    }

    fn observe_trap(&self, event: TrapEvent) {
        if let Some(console) = CONSOLE.get() {
            let _ = console.try_write_fmt(format_args!(
                "@@RUSTSBI_MTEST type=UNEXPECTED_TRAP event={event:?}\n"
            ));
        }
        crate::trap::abort()
    }
}

/// Complete dedicated-runner state after production machine initialization.
pub type MachineTests = Tests<BootInfo>;

/// Prepares production machine state, parks peer harts, and validates tests.
pub fn prepare(boot: BootInfo, console: Console) -> MachineTests {
    CONSOLE.call_once(|| console);
    let prepared = crate::boot::prepare_runtime(boot, Box::new(UnexpectedTrap));
    crate::startup::publish_runtime();
    while WARM_PARKED.load(Ordering::Acquire) < prepared.hart_count.saturating_sub(1) {
        core::hint::spin_loop();
    }
    match tests_from_linker(prepared.boot) {
        Ok(tests) => tests,
        Err(error) => {
            if let Some(console) = CONSOLE.get() {
                let _ = console.try_write_fmt(format_args!(
                    "@@RUSTSBI_MTEST type=REGISTRY_FAILURE error={error:?}\n"
                ));
            }
            crate::power::abort(|| {})
        }
    }
}

fn tests_from_linker(boot: BootInfo) -> Result<MachineTests, RegistryError> {
    unsafe extern "C" {
        static __mtest_start: u8;
        static __mtest_end: u8;
    }

    let start = core::ptr::addr_of!(__mtest_start) as usize;
    let end = core::ptr::addr_of!(__mtest_end) as usize;
    // The linker asserts this relation as an early diagnostic. The framework
    // repeats alignment, size, content, and uniqueness checks before it forms
    // a descriptor slice.
    if end
        .checked_sub(start)
        .is_none_or(|bytes| bytes == 0 || bytes % size_of::<machine_test::Descriptor>() != 0)
    {
        return Err(RegistryError::Bounds);
    }
    // SAFETY: the dedicated linker script retains only descriptor statics in
    // this bounded section; the framework validates the complete envelope.
    unsafe { MachineTests::from_linker_bounds(boot, start, end) }
}

pub(crate) fn mark_warm_parked() {
    WARM_PARKED.fetch_add(1, Ordering::Release);
}

pub(crate) fn warm_parked_count() -> usize {
    WARM_PARKED.load(Ordering::Acquire)
}

pub(crate) fn console() -> Option<&'static Console> {
    CONSOLE.get()
}
