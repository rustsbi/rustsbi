//! Chapter 5. Legacy Extensions (EIDs #0x00 - #0x0F)

pub use sbi_spec::legacy::*;

/// §5.1
#[deprecated = "replaced by `set_timer` from Timer extension"]
#[inline]
pub fn set_timer(stime_value: u64) -> usize {
    match () {
        #[cfg(target_pointer_width = "32")]
        () => sbi_call_legacy_2(LEGACY_SET_TIMER, stime_value as _, (stime_value >> 32) as _),
        #[cfg(target_pointer_width = "64")]
        () => sbi_call_legacy_1(LEGACY_SET_TIMER, stime_value as _),
    }
}

/// §5.2
///
/// No replacement.
#[deprecated = "no replacement"]
#[inline]
pub fn console_putchar(c: usize) -> usize {
    sbi_call_legacy_1(LEGACY_CONSOLE_PUTCHAR, c)
}

/// §5.3
///
/// No replacement.
#[deprecated = "no replacement"]
#[inline]
pub fn console_getchar() -> usize {
    sbi_call_legacy_0(LEGACY_CONSOLE_GETCHAR)
}

/// §5.4
///
/// No replacement. Just clear `sip.SSIP` directly.
#[deprecated = "you can clear `sip.SSIP` CSR bit directly"]
#[inline]
pub fn clear_ipi() -> usize {
    sbi_call_legacy_0(LEGACY_CLEAR_IPI)
}

/// §5.5
///
/// Replaced by [`send_ipi`](super::send_ipi) from [`sPI`](super::EID_SPI) extension.
#[deprecated = "replaced by `send_ipi` from `sPI` extension"]
#[inline]
pub fn send_ipi(hart_mask: usize) -> usize {
    sbi_call_legacy_1(LEGACY_SEND_IPI, hart_mask)
}

/// §5.6
///
/// Replaced by [`remote_fence_i`](super::remote_fence_i) from [`RFNC`](super::EID_RFNC) extension.
#[deprecated = "replaced by `remote_fence_i` from `RFNC` extension"]
#[inline]
pub fn remote_fence_i(hart_mask: usize) -> usize {
    sbi_call_legacy_1(LEGACY_REMOTE_FENCE_I, hart_mask)
}

/// §5.7
///
/// Replaced by [`remote_sfence_vma`](super::remote_sfence_vma) from [`RFNC`](super::EID_RFNC) extension.
#[deprecated = "replaced by `remote_sfence_vma` from `RFNC` extension"]
#[inline]
pub fn remote_fence_vma(hart_mask: usize, start: usize, size: usize) -> usize {
    sbi_call_legacy_3(LEGACY_REMOTE_SFENCE_VMA, hart_mask, start, size)
}

/// §5.8
///
/// Replaced by [`remote_sfence_vma_asid`](super::remote_sfence_vma_asid) from [`RFNC`](super::EID_RFNC) extension.
#[deprecated = "replaced by `remote_sfence_vma_asid` from `RFNC` extension"]
#[inline]
pub fn remote_fence_vma_asid(hart_mask: usize, start: usize, size: usize, asid: usize) -> usize {
    sbi_call_legacy_4(LEGACY_REMOTE_SFENCE_VMA_ASID, hart_mask, start, size, asid)
}

/// §5.9
///
/// Replaced by [`system_reset`](super::system_reset) in the [`SRST`](super::EID_SRST) extension.
#[deprecated = "replaced by `system_reset` from System `SRST` extension"]
#[inline]
pub fn shutdown() -> ! {
    sbi_call_legacy_0(LEGACY_SHUTDOWN);
    unreachable!()
}

#[inline(always)]
fn sbi_call_legacy_0(eid: usize) -> usize {
    let error;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") eid,
            lateout("a0") error,
        );
    }
    error
}

#[inline(always)]
fn sbi_call_legacy_1(eid: usize, arg0: usize) -> usize {
    let error;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") eid,
            inlateout("a0") arg0 => error,
        );
    }
    error
}

#[cfg(target_pointer_width = "32")]
#[inline(always)]
fn sbi_call_legacy_2(eid: usize, arg0: usize, arg1: usize) -> usize {
    let error;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") eid,
            inlateout("a0") arg0 => error,
            in("a1") arg1,
        );
    }
    error
}

#[inline(always)]
fn sbi_call_legacy_3(eid: usize, arg0: usize, arg1: usize, arg2: usize) -> usize {
    let error;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") eid,
            inlateout("a0") arg0 => error,
            in("a1") arg1,
            in("a2") arg2,
        );
    }
    error
}

#[inline(always)]
fn sbi_call_legacy_4(eid: usize, arg0: usize, arg1: usize, arg2: usize, arg3: usize) -> usize {
    let error;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") eid,
            inlateout("a0") arg0 => error,
            in("a1") arg1,
            in("a2") arg2,
            in("a3") arg3,
        );
    }
    error
}
