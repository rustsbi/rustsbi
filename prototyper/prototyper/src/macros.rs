#[allow(unused)]
macro_rules! print {
    ($($arg:tt)*) => {
        use core::fmt::Write;
        if unsafe {$crate::platform::PLATFORM.have_console()} {
            let console = unsafe { $crate::platform::PLATFORM.sbi.console.as_mut().unwrap() };
            console.write_fmt(core::format_args!($($arg)*)).unwrap();
            drop(console);
        }
    }
}

#[allow(unused)]
macro_rules! println {
    () => ($crate::print!("\n\r"));
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        if unsafe {$crate::platform::PLATFORM.have_console()} {
            let console = unsafe { $crate::platform::PLATFORM.sbi.console.as_mut().unwrap() };
            console.write_fmt(core::format_args!($($arg)*)).unwrap();
            console.write_str("\n\r").unwrap();
        }
    }}
}

#[allow(unused)]
macro_rules! has_csr {
    ($($x: expr)*) => {{
            use core::arch::asm;
            use riscv::register::mtvec;
            use crate::sbi::early_trap::light_expected_trap;
            let res: usize;
            unsafe {
                // Backup old mtvec
                let mtvec = mtvec::read().bits();
                // Write expected_trap
                mtvec::write(light_expected_trap as _, mtvec::TrapMode::Direct);
                asm!("addi a0, zero, 0",
                    "addi a1, zero, 0",
                    "csrr a2, {}",
                    "mv {}, a0",
                    const $($x)*,
                    out(reg) res,
                    options(nomem));
                asm!("csrw mtvec, {}", in(reg) mtvec);
            }
            res == 0
    }};
}
