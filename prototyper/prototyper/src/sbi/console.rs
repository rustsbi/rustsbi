use alloc::boxed::Box;
use core::fmt::{self, Write};
use rustsbi::{Console, Physical, SbiRet};
use spin::Mutex;

use crate::platform::PLATFORM;

// Returns the 64-bit physical address composed from the low/high parts.
#[inline]
fn physical_addr(lo: usize, hi: usize) -> u64 {
    ((hi as u64) << 32) | (lo as u64)
}

// Checks whether the physical range is within platform DRAM and directly addressable.
#[inline]
fn is_physical_range_valid(start: u64, len: usize) -> bool {
    if start > usize::MAX as u64 {
        return false;
    }

    let Some(end) = start.checked_add(len as u64) else {
        return false;
    };

    let memory_range = unsafe { PLATFORM.info.memory_range.as_ref() };

    match memory_range {
        Some(range) => start >= range.start as u64 && end <= range.end as u64,
        None => false,
    }
}

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
pub struct SbiConsole {
    inner: Mutex<Box<dyn ConsoleDevice>>,
}

impl SbiConsole {
    /// Creates a new SBI console that wraps the provided locked console device.
    ///
    /// # Arguments
    /// * `inner` - A mutex containing the console device implementation
    #[inline]
    pub fn new(inner: Mutex<Box<dyn ConsoleDevice>>) -> Self {
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

impl Console for SbiConsole {
    /// Write a physical memory buffer to the console.
    #[inline]
    fn write(&self, bytes: Physical<&[u8]>) -> SbiRet {
        let len = bytes.num_bytes();
        if len == 0 {
            return SbiRet::success(0);
        }
        let start = physical_addr(bytes.phys_addr_lo(), bytes.phys_addr_hi());
        if !is_physical_range_valid(start, len) {
            return SbiRet::invalid_param();
        }
        let buf = unsafe { core::slice::from_raw_parts(start as *const u8, len) };
        let bytes_written = self.inner.lock().write(buf);
        SbiRet::success(bytes_written)
    }

    /// Read from console into a physical memory buffer.
    #[inline]
    fn read(&self, bytes: Physical<&mut [u8]>) -> SbiRet {
        let len = bytes.num_bytes();
        if len == 0 {
            return SbiRet::success(0);
        }
        let start = physical_addr(bytes.phys_addr_lo(), bytes.phys_addr_hi());
        if !is_physical_range_valid(start, len) {
            return SbiRet::invalid_param();
        }
        let buf = unsafe { core::slice::from_raw_parts_mut(start as *mut u8, len) };
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

impl fmt::Write for SbiConsole {
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
    unsafe { PLATFORM.sbi.console.as_mut().unwrap().putchar(c) }
}

/// Global function to read a character from the console.
#[inline]
pub fn getchar() -> usize {
    unsafe { PLATFORM.sbi.console.as_mut().unwrap().getchar() }
}
