// Ref: rcore-console crate

#[allow(unused)]
macro_rules! print {
    ($($arg:tt)*) => {
        let mut console = $crate::console::CONSOLE.lock();
        console.write_fmt(core::format_args!($($arg)*)).unwrap();
        drop(console);
    }
}

#[allow(unused)]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => {{
        let mut console = $crate::console::CONSOLE.lock();
        console.write_fmt(core::format_args!($($arg)*)).unwrap();
        console.write_char('\n').unwrap();
        drop(console);
    }}
}
