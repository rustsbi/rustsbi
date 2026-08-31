use core::sync::atomic::fence;

use rustsbi::SbiRet;
use sbi_spec::binary::SharedPtr;
use spin::Mutex;

use crate::cfg::NUM_HART_MAX;
use crate::riscv::current_hartid;

#[repr(C)]
struct StaShmemData {
    sequence: u32,
    flags: u32,
    steal: u64,
    preempted: u8,
    pad: [u8; 47],
}

const _: () = assert!(core::mem::size_of::<StaShmemData>() == 64);

struct StaShmem {
    lo: usize,
    hi: usize,
}

impl StaShmem {
    const DISABLED: Self = Self {
        lo: usize::MAX,
        hi: usize::MAX,
    };

    fn store(slot: &Mutex<Self>, lo: usize, hi: usize) {
        let mut shmem = slot.lock();
        shmem.lo = lo;
        shmem.hi = hi;
    }
}

// The Prototyper uses XLEN-sized identity-mapped physical addresses. Both
// parts are retained for the SBI-defined disabled value and future wider
// address support, but only a zero high part can be dereferenced here.
static STA_SHMEM: [Mutex<StaShmem>; NUM_HART_MAX] =
    [const { Mutex::new(StaShmem::DISABLED) }; NUM_HART_MAX];

unsafe fn zero_shmem(addr: usize) {
    // SAFETY: the caller validated the complete shared-memory range.
    unsafe {
        core::slice::from_raw_parts_mut(addr as *mut u8, core::mem::size_of::<StaShmemData>())
            .fill(0);
    }
    fence(core::sync::atomic::Ordering::SeqCst);
}

/// Steal-time Accounting extension using supervisor-provided shared memory.
pub(crate) struct SbiSta;

impl rustsbi::Sta for SbiSta {
    fn set_shmem(&self, shmem: SharedPtr<[u8; 64]>, flags: usize) -> SbiRet {
        if flags != 0 {
            return SbiRet::invalid_param();
        }

        let lo = shmem.phys_addr_lo();
        let hi = shmem.phys_addr_hi();

        // All-ones shared pointer disables steal-time reporting.
        if hi == usize::MAX && lo == usize::MAX {
            StaShmem::store(&STA_SHMEM[current_hartid()], lo, hi);
            return SbiRet::success(0);
        }

        if lo & 0x3f != 0 {
            return SbiRet::invalid_param();
        }
        // The firmware's physical-memory validation and identity mapping use
        // XLEN-sized addresses, so a non-zero upper part is not dereferenceable.
        if hi != 0 {
            return SbiRet::invalid_address();
        }

        if !crate::firmware::supervisor_writable(lo, 64) {
            return SbiRet::invalid_address();
        }

        // The Prototyper has no scheduler that can produce stolen time. A
        // zeroed structure therefore reports the only state it can guarantee.
        // SAFETY: the validated 64-byte range is writable and lies outside
        // firmware memory.
        unsafe {
            zero_shmem(lo);
        }

        StaShmem::store(&STA_SHMEM[current_hartid()], lo, hi);
        SbiRet::success(0)
    }
}
