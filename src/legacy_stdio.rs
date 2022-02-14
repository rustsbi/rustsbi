//! 这个模块的两个宏应该公开
//! 如果制造实例的时候，给定了stdout，那么就会打印到这个stdout里面
use crate::util::AmoMutex;
use alloc::boxed::Box;
use embedded_hal::serial::{Read, Write};
use nb::block;

/// Legacy standard input/output
pub trait LegacyStdio: Send {
    /// Get a character from legacy stdin
    fn getchar(&mut self) -> u8;
    /// Put a character into legacy stdout
    fn putchar(&mut self, ch: u8);
}

/// Use serial in `embedded-hal` as legacy standard input/output
struct EmbeddedHalSerial<T> {
    inner: T,
}

impl<T> EmbeddedHalSerial<T> {
    /// Create a wrapper with a value
    #[inline]
    fn new(inner: T) -> Self {
        Self { inner }
    }
}

impl<T: Send> LegacyStdio for EmbeddedHalSerial<T>
where
    T: Read<u8> + Write<u8>,
{
    #[inline]
    fn getchar(&mut self) -> u8 {
        // 直接调用embedded-hal里面的函数
        // 关于unwrap：因为这个是legacy函数，这里没有详细的处理流程，就panic掉
        block!(self.inner.read()).ok().unwrap()
    }

    #[inline]
    fn putchar(&mut self, ch: u8) {
        // 直接调用函数写一个字节
        block!(self.inner.write(ch)).ok();
        // 写一次flush一次，因为是legacy，就不考虑效率了
        block!(self.inner.flush()).ok();
    }
}

struct Fused<T, R>(T, R);

// 和上面的原理差不多，就是分开了
impl<T, R> LegacyStdio for Fused<T, R>
where
    T: Write<u8> + Send + 'static,
    R: Read<u8> + Send + 'static,
{
    #[inline]
    fn getchar(&mut self) -> u8 {
        block!(self.1.read()).ok().unwrap()
    }

    #[inline]
    fn putchar(&mut self, ch: u8) {
        block!(self.0.write(ch)).ok();
        block!(self.0.flush()).ok();
    }
}

static LEGACY_STDIO: AmoMutex<Option<Box<dyn LegacyStdio>>> = AmoMutex::new(None);

#[doc(hidden)] // use through a macro
pub fn init_legacy_stdio_embedded_hal<T: Read<u8> + Write<u8> + Send + 'static>(serial: T) {
    let serial = EmbeddedHalSerial::new(serial);
    *LEGACY_STDIO.lock() = Some(Box::new(serial));
}

#[doc(hidden)] // use through a macro
pub fn init_legacy_stdio_embedded_hal_fuse<T, R>(tx: T, rx: R)
where
    T: Write<u8> + Send + 'static,
    R: Read<u8> + Send + 'static,
{
    let serial = Fused(tx, rx);
    *LEGACY_STDIO.lock() = Some(Box::new(serial));
}

#[inline]
pub fn legacy_stdio_putchar(ch: u8) {
    if let Some(stdio) = LEGACY_STDIO.lock().as_mut() {
        stdio.putchar(ch)
    }
}

#[inline]
pub fn legacy_stdio_getchar() -> usize {
    if let Some(stdio) = LEGACY_STDIO.lock().as_mut() {
        stdio.getchar() as usize
    } else {
        // According to RISC-V SBI spec 0.3.1-rc1, Section 4.3, this function returns -1
        // when fails to read from debug console. Thank you @duskmoon314
        usize::from_ne_bytes(isize::to_ne_bytes(-1))
    }
}

use core::fmt;

struct Stdout;

impl fmt::Write for Stdout {
    #[inline]
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if let Some(stdio) = LEGACY_STDIO.lock().as_mut() {
            for byte in s.as_bytes() {
                stdio.putchar(*byte)
            }
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
#[macro_export(local_inner_macros)]
macro_rules! print {
    ($($arg:tt)*) => ({
        $crate::legacy_stdio::_print(core::format_args!($($arg)*));
    });
}

/// Prints to the legacy debug console, with a newline.
///
/// This is only supported when there exists legacy extension;
/// otherwise platform caller should use an early kernel input/output device
/// declared in platform specific hardware.
#[macro_export(local_inner_macros)]
macro_rules! println {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::legacy_stdio::_print(core::format_args!(core::concat!($fmt, "\r\n") $(, $($arg)+)?));
    }
}
