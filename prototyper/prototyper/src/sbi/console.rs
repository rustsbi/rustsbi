use alloc::boxed::Box;
use core::fmt;
use rustsbi::{Console, Physical, SbiRet};
use spin::Mutex;

use crate::platform::PLATFORM;

// Checks whether `(phys_addr_lo, phys_addr_hi, len)` can be represented
// as a native address range on this machine.
//
// See the shared-memory physical address rules in
// <https://github.com/riscv-non-isa/riscv-sbi-doc/blob/v3.0/src/binary-encoding.adoc>
// and the DBCN call definitions in
// <https://github.com/riscv-non-isa/riscv-sbi-doc/blob/v3.0/src/ext-debug-console.adoc>.
#[inline]
fn checked_physical_addr(lo: usize, hi: usize, len: usize) -> Result<usize, SbiRet> {
    // If the address exceeds our implementation's native `usize` capacity,
    // return SBI_ERR_FAILED as we are enforcing a stricter range limit.
    if hi != 0 {
        return Err(SbiRet::failed());
    }

    // Check for native usize overflow. If it overflows, it's definitively an invalid address.
    let _end = lo.checked_add(len).ok_or_else(SbiRet::invalid_address)?;

    Ok(lo)
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
        let byte = [c as u8];
        while self.inner.lock().write(&byte) == 0 {
            core::hint::spin_loop();
        }
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
        while self.inner.lock().read(core::slice::from_mut(&mut c)) == 0 {
            core::hint::spin_loop();
        }
        c as usize
    }

    // Rejects buffers that this firmware cannot safely turn into raw slices.
    //
    // The SBI address tuple may still be valid,
    // but this implementation only accepts buffers inside `PLATFORM.info.memory_range`.
    #[inline]
    fn checked_physical_buffer<P>(&self, bytes: &Physical<P>) -> Result<(usize, usize), SbiRet> {
        let len = bytes.num_bytes();
        if len == 0 {
            return Ok((0, 0));
        }

        let start = match checked_physical_addr(bytes.phys_addr_lo(), bytes.phys_addr_hi(), len) {
            Ok(start) => start,
            Err(err) => return Err(err),
        };

        match unsafe { PLATFORM.info.memory_range.as_ref() } {
            Some(range)
                if start >= range.start
                    && start.checked_add(len).is_some_and(|end| end <= range.end) => {}
            _ => return Err(SbiRet::failed()),
        }

        Ok((start, len))
    }
}

impl Console for SbiConsole {
    /// Writes bytes from the physical buffer described by `bytes`.
    #[inline]
    fn write(&self, bytes: Physical<&[u8]>) -> SbiRet {
        let (start, len) = match self.checked_physical_buffer(&bytes) {
            Ok(buf) => buf,
            Err(err) => return err,
        };
        if len == 0 {
            return SbiRet::success(0);
        }

        // SAFETY: `checked_physical_buffer` only returns ranges that
        // were accepted as representable and within `memory_range`.
        let buf = unsafe { core::slice::from_raw_parts(start as *const u8, len) };
        let bytes_written = self.inner.lock().write(buf);
        SbiRet::success(bytes_written)
    }

    /// Reads bytes into the physical buffer described by `bytes`.
    #[inline]
    fn read(&self, bytes: Physical<&mut [u8]>) -> SbiRet {
        let (start, len) = match self.checked_physical_buffer(&bytes) {
            Ok(buf) => buf,
            Err(err) => return err,
        };
        if len == 0 {
            return SbiRet::success(0);
        }

        // SAFETY: `checked_physical_buffer` only returns ranges that
        // were accepted as representable and within `memory_range`.
        let buf = unsafe { core::slice::from_raw_parts_mut(start as *mut u8, len) };
        let bytes_read = self.inner.lock().read(buf);
        SbiRet::success(bytes_read)
    }

    /// Writes `byte` to the console.
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
        while !bytes.is_empty() {
            let count = self.inner.lock().write(bytes);
            if count == 0 {
                return Err(fmt::Error);
            }
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
