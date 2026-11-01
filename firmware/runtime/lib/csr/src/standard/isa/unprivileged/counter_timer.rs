//! Zicntr counters and timers.
use crate::macros::csr;
use crate::values::counter::CounterValue;

csr! {
    /// Architectural cycle counter (`cycle`).
    /// See <https://docs.riscv.org/reference/isa/unpriv/counters.html>.
    Cycle { 0xc00, r: CounterValue }
    /// Upper half of `cycle` on RV32.
    /// See <https://docs.riscv.org/reference/isa/unpriv/counters.html>.
    CycleHigh { 0xc80, r: CounterValue }
    /// Architectural time counter (`time`).
    ///
    /// See <https://docs.riscv.org/reference/isa/unpriv/counters.html>.
    Time { 0xc01, r: CounterValue }
    /// Upper half of the architectural time counter on RV32 (`timeh`).
    ///
    /// See <https://docs.riscv.org/reference/isa/unpriv/counters.html>.
    TimeHigh { 0xc81, r: CounterValue }
    /// Architectural retired-instruction counter (`instret`).
    /// See <https://docs.riscv.org/reference/isa/unpriv/counters.html>.
    InstructionsRetired { 0xc02, r: CounterValue }
    /// Upper half of `instret` on RV32.
    /// See <https://docs.riscv.org/reference/isa/unpriv/counters.html>.
    InstructionsRetiredHigh { 0xc82, r: CounterValue }
}

macro_rules! read_64 {
    ($name:ident, $high:ident) => {
        impl $name {
            /// Reads a consistent 64-bit counter value on RV32 or RV64.
            #[inline]
            pub fn read64() -> u64 {
                #[cfg(target_pointer_width = "64")]
                {
                    usize::from(Self::read()) as u64
                }
                #[cfg(target_pointer_width = "32")]
                loop {
                    let high = usize::from($high::read());
                    let low = usize::from(Self::read());
                    if high == usize::from($high::read()) {
                        return ((high as u64) << 32) | low as u64;
                    }
                }
            }
        }
    };
}

read_64!(Cycle, CycleHigh);
read_64!(Time, TimeHigh);
read_64!(InstructionsRetired, InstructionsRetiredHigh);

/// User-visible performance counter `hpmcounter3` through `hpmcounter31`.
/// See <https://docs.riscv.org/reference/isa/unpriv/counters.html>.
pub enum PerformanceCounter<const INDEX: u8> {}

impl<const INDEX: u8> PerformanceCounter<INDEX> {
    /// Architectural CSR number for counter indices 3 through 31.
    pub const NUMBER: u16 = {
        assert!(INDEX >= 3 && INDEX <= 31, "counter index must be 3..=31");
        0xc00 + INDEX as u16
    };

    /// Reads a consistent 64-bit value on RV32 or RV64.
    #[inline]
    pub fn read64() -> u64 {
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            unimplemented!("CSR access requires RISC-V")
        }
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            let read = || {
                let bits: usize;
                // SAFETY: Counter reads have no write effects; inaccessible CSRs trap.
                unsafe {
                    core::arch::asm!("csrr {bits}, {csr}", bits = out(reg) bits,
                    csr = const Self::NUMBER, options(nostack));
                }
                bits
            };
            #[cfg(target_pointer_width = "64")]
            {
                read() as u64
            }
            #[cfg(target_pointer_width = "32")]
            loop {
                let read_high = || {
                    let bits: usize;
                    // SAFETY: The upper counter CSR is read-only and may trap if absent.
                    unsafe {
                        core::arch::asm!("csrr {bits}, {csr}", bits = out(reg) bits,
                        csr = const Self::NUMBER + 0x80, options(nostack));
                    }
                    bits
                };
                let high = read_high();
                let low = read();
                if high == read_high() {
                    return ((high as u64) << 32) | low as u64;
                }
            }
        }
    }
}
