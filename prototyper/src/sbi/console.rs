use crate::board::BOARD;
use core::fmt::{self, Write};
use rustsbi::{Console, Physical, SbiRet};
use spin::Mutex;

/// A trait that must be implemented by console devices to provide basic I/O functionality.
pub trait ConsoleDevice {
    /// Reads bytes from the console into the provided buffer.
    ///
    /// # Returns
    /// The number of bytes that were successfully read.
    fn read(&self, buf: &mut [u8]) -> usize;

    /// Writes bytes from the provided buffer to the console.
    ///
    /// # Returns
    /// The number of bytes that were successfully written.
    fn write(&self, buf: &[u8]) -> usize;
}

/// An implementation of the SBI console interface that wraps a console device.
///
/// This provides a safe interface for interacting with console hardware through the
/// SBI specification.
pub struct SbiConsole<'a, T: ConsoleDevice> {
    inner: &'a Mutex<T>,
}

impl<'a, T: ConsoleDevice> SbiConsole<'a, T> {
    /// Creates a new SBI console that wraps the provided locked console device.
    ///
    /// # Arguments
    /// * `inner` - A mutex containing the console device implementation
    #[inline]
    pub fn new(inner: &'a Mutex<T>) -> Self {
        Self { inner }
    }

    /// Writes a single character to the console.
    ///
    /// # Arguments
    /// * `c` - The character to write, as a usize
    ///
    /// # Returns
    /// Always returns 0 to indicate success
    #[inline]
    pub fn putchar(&mut self, c: usize) -> usize {
        self.write_char(c as u8 as char).unwrap();
        0
    }

    /// Reads a single character from the console.
    ///
    /// This method will block until a character is available to be read.
    ///
    /// # Returns
    /// The read character as a usize
    #[inline]
    pub fn getchar(&self) -> usize {
        let mut c = 0u8;
        let console = self.inner.lock();
        // Block until we successfully read 1 byte
        while console.read(core::slice::from_mut(&mut c)) != 1 {
            core::hint::spin_loop();
        }
        c as usize
    }
}

impl<'a, T: ConsoleDevice> Console for SbiConsole<'a, T> {
    /// Write a physical memory buffer to the console.
    #[inline]
    fn write(&self, bytes: Physical<&[u8]>) -> SbiRet {
        // TODO: verify valid memory range for a `Physical` slice.
        let start = bytes.phys_addr_lo();
        let buf = unsafe { core::slice::from_raw_parts(start as *const u8, bytes.num_bytes()) };
        let bytes_written = self.inner.lock().write(buf);
        SbiRet::success(bytes_written)
    }

    /// Read from console into a physical memory buffer.
    #[inline]
    fn read(&self, bytes: Physical<&mut [u8]>) -> SbiRet {
        // TODO: verify valid memory range for a `Physical` slice.
        let start = bytes.phys_addr_lo();
        let buf = unsafe { core::slice::from_raw_parts_mut(start as *mut u8, bytes.num_bytes()) };
        let bytes_read = self.inner.lock().read(buf);
        SbiRet::success(bytes_read)
    }

    /// Write a single byte to the console.
    #[inline]
    fn write_byte(&self, byte: u8) -> SbiRet {
        self.inner.lock().write(&[byte]);
        SbiRet::success(0)
    }
}

impl<'a, T: ConsoleDevice> fmt::Write for SbiConsole<'a, T> {
    /// Implement Write trait for string formatting.
    #[inline]
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let mut bytes = s.as_bytes();
        let console = self.inner.lock();
        // Write all bytes in chunks
        while !bytes.is_empty() {
            let count = console.write(bytes);
            bytes = &bytes[count..];
        }
        Ok(())
    }
}

/// Global function to write a character to the console.
#[inline]
pub fn putchar(c: usize) -> usize {
    unsafe { BOARD.sbi.console.as_mut().unwrap().putchar(c) }
}

/// Global function to read a character from the console.
#[inline]
pub fn getchar() -> usize {
    unsafe { BOARD.sbi.console.as_mut().unwrap().getchar() }
}
