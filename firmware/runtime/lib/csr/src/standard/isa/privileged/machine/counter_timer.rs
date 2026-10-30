//! Machine performance counters.
//!
//! See <https://docs.riscv.org/reference/isa/priv/machine.html#_hardware_performance_monitor>
//! and <https://docs.riscv.org/reference/isa/priv/sscofpmf.html>.
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
use crate::value::Value;
use crate::values::counter::CounterValue;
use crate::macros::csr;

csr! {
    /// Machine cycle counter.
    CycleCounter { 0xb00, r: CounterValue,
        /// # Safety
        /// Coordinate counter accounting with every user, including sibling
        /// harts when the implementation shares the cycle counter.
        unsafe w: CounterValue
    }
    /// Machine retired-instruction counter.
    InstructionCounter { 0xb02, r: CounterValue,
        /// # Safety
        /// Own this hart's counter accounting and coordinate its users.
        unsafe w: CounterValue
    }
    CycleCounterHigh { 0xb80, r: CounterValue,
        /// # Safety
        /// Coordinate both counter words and every user, including sibling
        /// harts when the implementation shares the cycle counter.
        unsafe w: CounterValue
    }
    InstructionCounterHigh { 0xb82, r: CounterValue,
        /// # Safety
        /// Own this hart's counter accounting and coordinate both words.
        unsafe w: CounterValue
    }
}

macro_rules! counter_register {
    ($(#[$attr:meta])* $name:ident, $base:literal) => {
        $(#[$attr])*
        pub enum $name<const INDEX: u8> {}

        $(#[$attr])*
        impl<const INDEX: u8> $name<INDEX> {
            /// Architectural CSR number; indices outside 3..=31 are rejected.
            pub const NUMBER: u16 = {
                assert!(INDEX >= 3 && INDEX <= 31, "counter index must be 3..=31");
                $base + INDEX as u16
            };

            /// Reads this hart's counter or event selector.
            ///
            /// Access to an unavailable or inaccessible register traps.
            #[inline]
            pub fn read() -> CounterValue {
                #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
                {
                    let bits: usize;
                    // SAFETY: These registers can be read without changing
                    // the execution environment. Inaccessible access traps.
                    unsafe {
                        core::arch::asm!("csrr {bits}, {csr}", bits = out(reg) bits,
                            csr = const Self::NUMBER, options(nostack));
                    }
                    CounterValue::from_bits(bits)
                }
                #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
                unimplemented!("CSR access requires RISC-V")
            }

            /// Writes this hart's counter or event selector.
            ///
            /// Access to an unavailable or inaccessible register traps.
            ///
            /// # Safety
            ///
            /// Execute on the intended hart in M-mode with a valid trap context.
            /// Coordinate counter and event-selector updates with their users.
            #[inline]
            pub unsafe fn write(value: CounterValue) {
                #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
                // SAFETY: The caller coordinates the update with the users
                // of this counter and its event selector.
                unsafe {
                    core::arch::asm!("csrw {csr}, {bits}", csr = const Self::NUMBER,
                        bits = in(reg) usize::from(value), options(nostack));
                }
                #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
                {
                    let _ = value;
                    unimplemented!("CSR access requires RISC-V");
                }
            }
        }
    };
}

counter_register!(
    /// Machine performance counter indexed from 3 through 31.
    PerformanceCounter, 0xb00
);
counter_register!(
    /// Upper word of a machine performance counter on RV32.
    PerformanceCounterHigh, 0xb80
);
counter_register!(
    /// Platform-defined event selector for a programmable machine counter.
    PerformanceEvent, 0x320
);
counter_register!(
    /// Optional upper event-selector word on RV32 (Sscofpmf).
    PerformanceEventHigh, 0x720
);
