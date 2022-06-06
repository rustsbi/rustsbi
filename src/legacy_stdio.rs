//! 这个模块的两个宏应该公开
//! 如果制造实例的时候，给定了stdout，那么就会打印到这个stdout里面

use crate::util::AmoOnceRef;
use core::fmt;

/// Legacy standard input/output
pub trait LegacyStdio: Send + Sync {
    /// Get a character from legacy stdin
    fn getchar(&self) -> u8;
    /// Put a character into legacy stdout
    fn putchar(&self, ch: u8);

    fn write_str(&self, s: &str) {
        for byte in s.as_bytes() {
            self.putchar(*byte)
        }
    }
}

static STDIO: AmoOnceRef<dyn LegacyStdio> = AmoOnceRef::new();

#[inline]
pub fn init_legacy_stdio(stdio: &'static dyn LegacyStdio) {
    if !STDIO.try_call_once(stdio) {
        panic!("load sbi module when already loaded")
    }
}

#[inline]
pub fn legacy_stdio_putchar(ch: u8) {
    if let Some(stdio) = STDIO.get() {
        stdio.putchar(ch)
    }
}

#[inline]
pub fn legacy_stdio_getchar() -> usize {
    if let Some(stdio) = STDIO.get() {
        stdio.getchar() as usize
    } else {
        // According to RISC-V SBI spec 0.3.1-rc1, Section 4.3, this function returns -1
        // when fails to read from debug console. Thank you @duskmoon314
        usize::from_ne_bytes(isize::to_ne_bytes(-1))
    }
}

struct Stdout;

impl fmt::Write for Stdout {
    #[inline]
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if let Some(stdio) = STDIO.get() {
            stdio.write_str(s);
        }
        Ok(())
    }
}

#[inline]
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use fmt::Write;
    Stdout.write_fmt(args).unwrap();
}

/// Prints to the legacy debug console.
///
/// This is only supported when there exists legacy extension;
/// otherwise platform caller should use an early kernel input/output device
/// declared in platform specific hardware.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::legacy_stdio::_print(core::format_args!($($arg)*)));
}

/// Prints to the legacy debug console, with a newline.
///
/// This is only supported when there exists legacy extension;
/// otherwise platform caller should use an early kernel input/output device
/// declared in platform specific hardware.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\r\n"));
    ($($arg:tt)*) => {
        $crate::legacy_stdio::_print(core::format_args!($($arg)*));
        $crate::print!("\r\n");
    }
}
