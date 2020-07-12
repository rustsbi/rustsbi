//! 这个模块的两个宏应该公开
//! 如果制造实例的时候，给定了stdout，那么就会打印到这个stdout里面
use embedded_hal::serial::{Read, Write};
use nb::block;

/// Legacy standard input/output
pub trait LegacyStdio {
    /// Get a character from legacy stdin
    fn getchar(&mut self) -> u8;
    /// Put a character into legacy stdout
    fn putchar(&mut self, ch: u8);
}

/// Use serial in `embedded-hal` as legacy standard input/output
pub struct EmbeddedHalSerial<T> {
    inner: T,
}

impl<T> EmbeddedHalSerial<T> {
    /// Create a wrapper with a value
    pub fn new(inner: T) -> Self {
        Self { inner }
    }

    /// Unwrap the struct to get underlying value
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T> LegacyStdio for EmbeddedHalSerial<T>
where
    T: Read<u8> + Write<u8>,
{
    fn getchar(&mut self) -> u8 {
        // 直接调用embedded-hal里面的函数
        // 关于unwrap：因为这个是legacy函数，这里没有详细的处理流程，就panic掉
        block!(self.inner.try_read()).ok().unwrap()
    }

    fn putchar(&mut self, ch: u8) {
        // 直接调用函数写一个字节
        block!(self.inner.try_write(ch)).ok();
        // 写一次flush一次，因为是legacy，就不考虑效率了
        block!(self.inner.try_flush()).ok();
    }
}
