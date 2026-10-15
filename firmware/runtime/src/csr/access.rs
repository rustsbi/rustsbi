//! Private instructions for Runtime's concrete optional-hardware operations.

// Callers establish Runtime entry, M-mode, mscratch=0 and MPRV=0, and
// coordinate the selected register's effects. Each fault site covers only
// its own instruction; recovery does not emulate the register.
macro_rules! instruction {
    ($instruction:literal, $number:expr, $value:expr) => {{
        let instruction = || -> $crate::Result<usize> {
        const { assert!($number < 0x1000, "CSR number must fit in 12 bits"); }
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            let mut value: usize = $value;
            let cause: usize;
            // SAFETY: This private call site establishes the entry and
            // register-specific requirements described above.
            unsafe {
                core::arch::asm!(
                    "li t2, 0",
                    ".option push",
                    ".option norvc",
                    "2:",
                    $instruction,
                    "3:",
                    ".option pop",
                    crate::recovery::fault_site!(),
                    csr = const $number,
                    causes = const 1 << 2,
                    value = inout(reg) value,
                    out("t2") cause,
                    out("t3") _,
                );
            }
            if cause == 0 { Ok(value) } else { Err($crate::Error::IllegalInstruction) }
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = $value;
            crate::csr::unavailable()
        }
        };
        instruction()
    }};
}

pub(crate) use instruction;

#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
pub(crate) fn unavailable() -> ! {
    unimplemented!("CSR access requires RISC-V")
}
