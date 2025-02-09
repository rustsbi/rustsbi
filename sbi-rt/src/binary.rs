//! Chapter 3. Binary Encoding
// This module is designated to use under RISC-V only, but it builds under non-RISC-V targets
// to allow unit tests and `cargo fix` operations.

// `sbi_call_6` has 8 arguments which is allowed
#![allow(clippy::too_many_arguments)]

use sbi_spec::binary::SbiRet;

#[inline(always)]
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(crate) fn sbi_call_0(eid: usize, fid: usize) -> SbiRet {
    let (error, value);
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") eid,
            in("a6") fid,
            lateout("a0") error,
            lateout("a1") value,
        );
    }
    SbiRet { error, value }
}

#[inline(always)]
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
pub(crate) fn sbi_call_0(_eid: usize, _fid: usize) -> SbiRet {
    unimplemented!("unsupported architecture")
}

#[inline(always)]
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(crate) fn sbi_call_1(eid: usize, fid: usize, arg0: usize) -> SbiRet {
    let (error, value);
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") eid,
            in("a6") fid,
            inlateout("a0") arg0 => error,
            lateout("a1") value,
        );
    }
    SbiRet { error, value }
}

#[inline(always)]
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
pub(crate) fn sbi_call_1(_eid: usize, _fid: usize, _arg0: usize) -> SbiRet {
    unimplemented!("unsupported architecture")
}

#[inline(always)]
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(crate) fn sbi_call_2(eid: usize, fid: usize, arg0: usize, arg1: usize) -> SbiRet {
    let (error, value);
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") eid,
            in("a6") fid,
            inlateout("a0") arg0 => error,
            inlateout("a1") arg1 => value,
        );
    }
    SbiRet { error, value }
}

#[inline(always)]
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
pub(crate) fn sbi_call_2(_eid: usize, _fid: usize, _arg0: usize, _arg1: usize) -> SbiRet {
    unimplemented!("unsupported architecture")
}

#[inline(always)]
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(crate) fn sbi_call_3(eid: usize, fid: usize, arg0: usize, arg1: usize, arg2: usize) -> SbiRet {
    let (error, value);
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") eid,
            in("a6") fid,
            inlateout("a0") arg0 => error,
            inlateout("a1") arg1 => value,
            in("a2") arg2,
        );
    }
    SbiRet { error, value }
}

#[inline(always)]
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
pub(crate) fn sbi_call_3(
    _eid: usize,
    _fid: usize,
    _arg0: usize,
    _arg1: usize,
    _arg2: usize,
) -> SbiRet {
    unimplemented!("unsupported architecture")
}

#[inline(always)]
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(crate) fn sbi_call_4(
    eid: usize,
    fid: usize,
    arg0: usize,
    arg1: usize,
    arg2: usize,
    arg3: usize,
) -> SbiRet {
    let (error, value);
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") eid,
            in("a6") fid,
            inlateout("a0") arg0 => error,
            inlateout("a1") arg1 => value,
            in("a2") arg2,
            in("a3") arg3,
        );
    }
    SbiRet { error, value }
}

#[inline(always)]
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
pub(crate) fn sbi_call_4(
    _eid: usize,
    _fid: usize,
    _arg0: usize,
    _arg1: usize,
    _arg2: usize,
    _arg3: usize,
) -> SbiRet {
    unimplemented!("unsupported architecture")
}

#[inline(always)]
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(crate) fn sbi_call_5(
    eid: usize,
    fid: usize,
    arg0: usize,
    arg1: usize,
    arg2: usize,
    arg3: usize,
    arg4: usize,
) -> SbiRet {
    let (error, value);
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") eid,
            in("a6") fid,
            inlateout("a0") arg0 => error,
            inlateout("a1") arg1 => value,
            in("a2") arg2,
            in("a3") arg3,
            in("a4") arg4,
        );
    }
    SbiRet { error, value }
}

#[inline(always)]
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
pub(crate) fn sbi_call_5(
    _eid: usize,
    _fid: usize,
    _arg0: usize,
    _arg1: usize,
    _arg2: usize,
    _arg3: usize,
    _arg4: usize,
) -> SbiRet {
    unimplemented!("unsupported architecture")
}

#[inline(always)]
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
#[allow(unused)] // only used on RV32 for RISC-V SBI 2.0 specification
pub(crate) fn sbi_call_6(
    eid: usize,
    fid: usize,
    arg0: usize,
    arg1: usize,
    arg2: usize,
    arg3: usize,
    arg4: usize,
    arg5: usize,
) -> SbiRet {
    let (error, value);
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") eid,
            in("a6") fid,
            inlateout("a0") arg0 => error,
            inlateout("a1") arg1 => value,
            in("a2") arg2,
            in("a3") arg3,
            in("a4") arg4,
            in("a5") arg5,
        );
    }
    SbiRet { error, value }
}

#[inline(always)]
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
#[allow(unused)] // only used on RV32 for RISC-V SBI 2.0 specification
pub(crate) fn sbi_call_6(
    _eid: usize,
    _fid: usize,
    _arg0: usize,
    _arg1: usize,
    _arg2: usize,
    _arg3: usize,
    _arg4: usize,
    _arg5: usize,
) -> SbiRet {
    unimplemented!("unsupported architecture")
}
