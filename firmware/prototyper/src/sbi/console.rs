//! Debug console and legacy byte I/O.
//!
//! # References
//!
//! - Specification: [RISC-V SBI v3.0, Section 12](https://docs.riscv.org/reference/sbi/_attachments/riscv-sbi.pdf#page=51) —
//!   DBCN transfers and function-specific error tables 50-52.

#![forbid(unsafe_code)]

use core::fmt;
use runtime::memory::{PhysAddr, PhysAddrRange, SupervisorMemory};
use runtime::rustsbi::{self, Physical, SbiRet};

use crate::driver::{Console, ConsoleError};

/// Maximum bytes transferred by one DBCN read or write.
const MAX_TRANSFER_BYTES: usize = 256;

/// SBI console service over a platform console device.
pub(crate) struct SbiConsole {
    console: &'static Console,
    supervisor_memory: &'static SupervisorMemory,
}

impl SbiConsole {
    /// Creates a new SBI console handle over the published console device.
    #[inline]
    pub(crate) fn new(
        console: &'static Console,
        supervisor_memory: &'static SupervisorMemory,
    ) -> Self {
        Self {
            console,
            supervisor_memory,
        }
    }

    // Console transfers require the complete buffer to belong to registered
    // supervisor memory. A zero-length transfer does not inspect its address.
    // SBI v3.0 DBCN Tables 50-51 assign INVALID_PARAM to buffers that do not
    // satisfy Section 3.2. This differs from Section 3.2's generic
    // INVALID_ADDRESS/FAILED guidance; this adapter follows the DBCN tables.
    #[inline]
    fn validate_buffer<P>(&self, buffer: &Physical<P>) -> Result<(PhysAddr, usize), SbiRet> {
        let start = PhysAddr::new(buffer.phys_addr_lo());
        let len = buffer.num_bytes();
        if len == 0 {
            return Ok((start, 0));
        }

        // This implementation cannot represent a physical address wider than
        // the native word.
        if buffer.phys_addr_hi() != 0 {
            return Err(SbiRet::invalid_param());
        }

        let range =
            PhysAddrRange::from_start_len(start, len).map_err(|_| SbiRet::invalid_param())?;
        if self.supervisor_memory.check_range(range).is_err() {
            return Err(SbiRet::invalid_param());
        }

        Ok((start, len))
    }
}

impl rustsbi::Console for SbiConsole {
    /// Writes bytes from the physical buffer described by `buffer`.
    #[inline]
    fn write(&self, buffer: Physical<&[u8]>) -> SbiRet {
        let (start, len) = match self.validate_buffer(&buffer) {
            Ok(buffer) => buffer,
            Err(error) => return error,
        };
        if len == 0 {
            return SbiRet::success(0);
        }

        // One ecall performs at most one bounded-size device operation and
        // reports any unconsumed suffix to the supervisor.
        let mut chunk = [0; MAX_TRANSFER_BYTES];
        let chunk_len = len.min(chunk.len());
        // Stage supervisor bytes locally so the device driver never receives a
        // caller-supplied physical pointer.
        if self
            .supervisor_memory
            .read(start, &mut chunk[..chunk_len])
            .is_err()
        {
            return SbiRet::invalid_param();
        }
        let count = match self.console.try_write(&chunk[..chunk_len]) {
            Ok(count) => count,
            Err(ConsoleError::Failed) => return SbiRet::failed(),
        };
        SbiRet::success(count)
    }

    /// Reads bytes into the physical buffer described by `buffer`.
    #[inline]
    fn read(&self, buffer: Physical<&mut [u8]>) -> SbiRet {
        let (start, len) = match self.validate_buffer(&buffer) {
            Ok(buffer) => buffer,
            Err(error) => return error,
        };
        if len == 0 {
            return SbiRet::success(0);
        }

        // As with writes, one ecall performs one bounded-size device operation
        // and reports the partial result directly.
        let mut chunk = [0; MAX_TRANSFER_BYTES];
        let chunk_len = len.min(chunk.len());
        let count = match self.console.try_read(&mut chunk[..chunk_len]) {
            Ok(count) => count,
            Err(ConsoleError::Failed) => return SbiRet::failed(),
        };
        // Leave bytes beyond the received prefix untouched.
        if count != 0
            && self
                .supervisor_memory
                .write(start, &chunk[..count])
                .is_err()
        {
            return SbiRet::invalid_param();
        }
        SbiRet::success(count)
    }

    /// Writes `byte` to the console.
    #[inline]
    fn write_byte(&self, byte: u8) -> SbiRet {
        match self.console.write_byte_blocking(byte) {
            Ok(()) => SbiRet::success(0),
            Err(ConsoleError::Failed) => SbiRet::failed(),
        }
    }
}

/// Prints formatted arguments to the console device, if one is present.
///
/// Used by the `print!` and `println!` macros. Retries until all bytes
/// have been written, spinning whenever a write returns zero.
/// Does nothing if no console device has been initialized.
/// Stops on device errors without recursively logging through the failed device.
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write as _;

    if let Some(mut console) = crate::platform::console_device() {
        let _ = console.write_fmt(args);
    }
}

#[macro_export]
#[allow(unused)]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::sbi::console::_print(core::format_args!($($arg)*))
    }
}

#[macro_export]
#[allow(unused)]
macro_rules! println {
    () => ($crate::print!("\n\r"));
    ($($arg:tt)*) => {{
        $crate::sbi::console::_print(core::format_args!($($arg)*));
        $crate::sbi::console::_print(core::format_args!("\n\r"));
    }}
}
