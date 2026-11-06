//! Debug console and legacy byte I/O.
//!
//! # References
//!
//! - Specification: [RISC-V SBI v3.0, Section 12](https://docs.riscv.org/reference/sbi/_attachments/riscv-sbi.pdf#page=51) —
//!   DBCN transfers and function-specific error tables 50-52.

#![forbid(unsafe_code)]

use core::fmt;
use runtime::{
    Error, Result, console,
    memory::{PhysAddr, PhysAddrRange, SupervisorMemory},
    rustsbi::{Console as SbiConsoleDevice, Physical, SbiRet},
};

// Bound stack use and work performed while the console device lock is held.
/// Maximum number of bytes per console operation.
const MAX_TRANSFER_BYTES: usize = 256;

/// Maps device errors to the errors supported by SBI DBCN.
#[inline]
fn console_error(error: Error) -> SbiRet {
    match error {
        Error::AccessDenied => SbiRet::denied(),
        _ => SbiRet::failed(),
    }
}

/// SBI console service over a platform console device.
pub(crate) struct SbiConsole {
    supervisor_memory: &'static SupervisorMemory,
}

impl SbiConsole {
    /// Creates an SBI console service for the supervisor memory.
    ///
    /// The Runtime console device must already be registered.
    #[inline]
    pub(crate) fn new(supervisor_memory: &'static SupervisorMemory) -> Self {
        Self { supervisor_memory }
    }

    // Console transfers require the complete buffer to belong to registered
    // supervisor memory; a zero-length transfer does not inspect its address.
    // SBI v3.0 DBCN tables 50–51 assign `SBI_ERR_INVALID_PARAM` to buffers
    // that do not satisfy Section 3.2.
    // This differs from Section 3.2's generic `SBI_ERR_INVALID_ADDRESS` and
    // `SBI_ERR_FAILED` guidance; this adapter follows the DBCN tables.
    #[inline]
    fn validate_buffer<P>(&self, buffer: &Physical<P>) -> Result<(PhysAddr, usize), SbiRet> {
        let start = PhysAddr::new(buffer.phys_addr_lo());
        let len = buffer.num_bytes();
        if len == 0 {
            return Ok((start, 0));
        }

        // The buffer base address is passed as two XLEN halves and may be
        // wider than XLEN bits. An address above the native physical-address
        // space cannot compose a registered supervisor-memory range. The DBCN
        // write and read error tables assign INVALID_PARAM to buffers that do
        // not satisfy the shared-memory address requirements, which is the
        // DBCN-specific equivalent of the generic INVALID_ADDRESS.
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

    pub(super) fn write_byte_blocking(&self, byte: u8) -> Result<()> {
        loop {
            let written = console::get()
                .ok_or(Error::NotReady)?
                .write_slice(&[byte])?;
            match written {
                0 => core::hint::spin_loop(),
                1 => return Ok(()),
                _ => return Err(Error::IoError),
            }
        }
    }
}

impl SbiConsoleDevice for SbiConsole {
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
        // DBCN keeps the input buffer stable for this synchronous transfer.
        if self
            .supervisor_memory
            .read(start, &mut chunk[..chunk_len])
            .is_err()
        {
            return SbiRet::invalid_param();
        }
        let Some(mut console) = console::get() else {
            return SbiRet::failed();
        };
        let result = console.write_slice(&chunk[..chunk_len]);
        let count = match result {
            Ok(count) => count,
            Err(error) => return console_error(error),
        };
        if count > chunk_len {
            return SbiRet::failed();
        }
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
        let Some(mut console) = console::get() else {
            return SbiRet::failed();
        };
        let result = console.read_slice(&mut chunk[..chunk_len]);
        let count = match result {
            Ok(count) => count,
            Err(error) => return console_error(error),
        };
        if count > chunk_len {
            return SbiRet::failed();
        }
        // DBCN reserves the output buffer for this synchronous transfer.
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
        match self.write_byte_blocking(byte) {
            Ok(()) => SbiRet::success(0),
            Err(error) => console_error(error),
        }
    }
}

/// Prints formatted arguments to the console device, if one is present.
///
/// Used by the `print!` and `println!` macros.
/// Retries until all bytes have been written,
/// spinning whenever a write returns zero.
/// Does nothing if no console device has been initialized.
/// Stops on backend errors without recursively logging through the failed device.
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write as _;

    struct ConsoleWriter;

    impl fmt::Write for ConsoleWriter {
        #[inline]
        fn write_str(&mut self, s: &str) -> fmt::Result {
            let bytes = s.as_bytes();
            let mut written = 0;
            while written < bytes.len() {
                let n = console::get()
                    .ok_or(fmt::Error)?
                    .write_slice(&bytes[written..])
                    .map_err(|_| fmt::Error)?;
                if n > bytes.len() - written {
                    return Err(fmt::Error);
                }
                if n == 0 {
                    core::hint::spin_loop();
                } else {
                    written += n;
                }
            }
            Ok(())
        }
    }

    if console::get().is_none() {
        return;
    }
    let _ = ConsoleWriter.write_fmt(args);
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
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => {{
        $crate::sbi::console::_print(core::format_args!($($arg)*));
        $crate::sbi::console::_print(core::format_args!("\n"));
    }}
}
