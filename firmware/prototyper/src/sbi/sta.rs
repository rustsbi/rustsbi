use core::sync::atomic::{AtomicUsize, Ordering};

use rustsbi::SbiRet;
use sbi_spec::binary::SharedPtr;

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
    lo: AtomicUsize,
    hi: AtomicUsize,
}

impl StaShmem {
    const DISABLED: Self = Self {
        lo: AtomicUsize::new(usize::MAX),
        hi: AtomicUsize::new(usize::MAX),
    };

    fn store(&self, lo: usize, hi: usize) {
        self.lo.store(lo, Ordering::Release);
        self.hi.store(hi, Ordering::Release);
    }
}

// The Prototyper uses XLEN-sized identity-mapped physical addresses. Both
// parts are retained for the SBI-defined disabled value and future wider
// address support, but only a zero high part can be dereferenced here.
static STA_SHMEM: [StaShmem; NUM_HART_MAX] = [const { StaShmem::DISABLED }; NUM_HART_MAX];

unsafe fn zero_shmem(addr: usize) {
    let ptr = addr as *mut u8;
    for offset in 0..core::mem::size_of::<StaShmemData>() {
        // SAFETY: the caller validated the complete shared-memory range.
        unsafe { ptr.add(offset).write_volatile(0) };
    }
    core::sync::atomic::fence(Ordering::SeqCst);
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
            STA_SHMEM[current_hartid()].store(lo, hi);
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

        STA_SHMEM[current_hartid()].store(lo, hi);
        SbiRet::success(0)
    }
}
