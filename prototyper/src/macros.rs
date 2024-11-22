// Ref: rcore-console crate

#[allow(unused)]
macro_rules! print {
    ($($arg:tt)*) => {
        use core::fmt::Write;
        let console = unsafe { $crate::board::BOARD.sbi.console.as_mut().unwrap() };
        console.write_fmt(core::format_args!($($arg)*)).unwrap();
        drop(console);
    }
}

#[allow(unused)]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let console = unsafe { $crate::board::BOARD.sbi.console.as_mut().unwrap() };
        console.write_fmt(core::format_args!($($arg)*)).unwrap();
        console.write_char('\n').unwrap();
    }}
}
