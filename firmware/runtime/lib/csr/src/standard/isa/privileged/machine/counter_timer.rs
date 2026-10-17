//! Machine performance counters.
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
use crate::value::Value;
use crate::values::counter::CounterValue;

/// Machine performance counter indexed from 3 through 31.
///
/// See
/// <https://docs.riscv.org/reference/isa/priv/machine.html#_hardware_performance_monitor>.
pub enum PerformanceCounter<const INDEX: u8> {}

impl<const INDEX: u8> PerformanceCounter<INDEX> {
    pub const NUMBER: u16 = {
        assert!(
            INDEX >= 3 && INDEX <= 31,
            "performance counter index must be 3..=31"
        );
        0xb00 + INDEX as u16
    };

    /// Reads this hart's counter. Unavailable access traps.
    pub fn read() -> CounterValue {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            let bits: usize;
            // SAFETY: Counter reads observe hardware without changing it.
            unsafe {
                core::arch::asm!("csrr {bits}, {csr}", bits = out(reg) bits,
                    csr = const Self::NUMBER, options(nostack));
            }
            CounterValue::from_bits(bits)
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        unimplemented!("CSR access requires RISC-V")
    }

    /// Writes this hart's counter. Unavailable access traps.
    ///
    /// # Safety
    ///
    /// Execute on the intended hart in M-mode with a valid trap context.
    /// Coordinate counter updates with its accounting owner.
    pub unsafe fn write(value: CounterValue) {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        // SAFETY: Counter writes change accounting, not the execution environment.
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
