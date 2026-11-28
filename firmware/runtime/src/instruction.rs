//! RISC-V instructions used by firmware for synchronization and idle waits.
//!
//! [FENCE](https://docs.riscv.org/reference/isa/v20260120/unpriv/rv32.html#fence),
//! [FENCE.I](https://docs.riscv.org/reference/isa/v20260120/unpriv/zifencei.html) and
//! [WFI](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#wfi).

use core::{marker::PhantomData, ops::BitOr};

#[cfg(target_arch = "riscv32")]
pub(crate) use core::arch::riscv32 as arch;
#[cfg(target_arch = "riscv64")]
pub(crate) use core::arch::riscv64 as arch;

/// A set of memory or I/O accesses ordered by `FENCE`.
pub trait AccessSet: Copy {
    /// The instruction's four-bit I/O/R/W mask.
    const BITS: u8;
}

/// A zero-sized access set whose mask is known at compile time.
#[derive(Clone, Copy)]
pub struct Access<const BITS: u8>;

/// The combination of access sets produced by `|`.
#[derive(Clone, Copy)]
pub struct Union<L, R>(PhantomData<(L, R)>);

impl<const BITS: u8> AccessSet for Access<BITS> {
    const BITS: u8 = BITS;
}

impl<L: AccessSet, R: AccessSet> AccessSet for Union<L, R> {
    const BITS: u8 = L::BITS | R::BITS;
}

impl<const BITS: u8, R: AccessSet> BitOr<R> for Access<BITS> {
    type Output = Union<Self, R>;

    #[inline(always)]
    fn bitor(self, _: R) -> Self::Output {
        Union(PhantomData)
    }
}

impl<L: AccessSet, R: AccessSet, S: AccessSet> BitOr<S> for Union<L, R> {
    type Output = Union<Self, S>;

    #[inline(always)]
    fn bitor(self, _: S) -> Self::Output {
        Union(PhantomData)
    }
}

/// Device inputs.
pub const I: Access<8> = Access;
/// Device outputs.
pub const O: Access<4> = Access;
/// Ordinary memory reads.
pub const R: Access<2> = Access;
/// Ordinary memory writes.
pub const W: Access<1> = Access;
/// All memory and device accesses.
pub const ALL: Access<15> = Access;

/// Orders this hart's selected predecessor accesses before its selected successor
/// accesses as observed by other harts and external devices.
///
/// For example, `fence(W, O)` orders memory writes before device outputs.
/// Combine access sets with `|`; both sets must be nonempty four-bit masks.
#[inline(always)]
pub fn fence<P: AccessSet, S: AccessSet>(_: P, _: S) {
    const {
        assert!(P::BITS > 0 && P::BITS < 16);
        assert!(S::BITS > 0 && S::BITS < 16);
    }
    // SAFETY: Both masks fit the FENCE immediate and fm is zero.
    unsafe {
        core::arch::asm!(
            ".insn i 0x0f, 0, x0, x0, {sets}",
            sets = const ((P::BITS as u16) << 4 | S::BITS as u16),
            options(nostack, preserves_flags),
        );
    }
}

/// Synchronizes instruction fetches on this hart with stores already visible to it.
///
/// This instruction traps if the hart does not implement Zifencei.
#[inline]
pub fn fence_i() {
    // SAFETY: FENCE.I synchronizes this hart's instruction fetches.
    unsafe { arch::fence_i() };
}

/// Hints that the hart may stall until an interrupt needs servicing.
///
/// WFI may resume for any reason, including immediately. Recheck the wake
/// condition after it returns. Restricted execution may raise an exception.
#[inline]
pub fn wait_for_interrupt() {
    // SAFETY: WFI only hints that this hart may stall.
    unsafe { arch::wfi() };
}
