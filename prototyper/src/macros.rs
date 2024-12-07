// Ref: rcore-console crate

#[allow(unused)]
macro_rules! print {
    ($($arg:tt)*) => {
        use core::fmt::Write;
        if unsafe {$crate::board::BOARD.have_console()} {
            let console = unsafe { $crate::board::BOARD.sbi.console.as_mut().unwrap() };
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
        if unsafe {$crate::board::BOARD.have_console()} {
            let console = unsafe { $crate::board::BOARD.sbi.console.as_mut().unwrap() };
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
            let res: usize;
            unsafe {
                // Backup old mtvec
                let mtvec = mtvec::read().bits();
                // Write expected_trap
                mtvec::write(expected_trap as _, mtvec::TrapMode::Direct);
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
