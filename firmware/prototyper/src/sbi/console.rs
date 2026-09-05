//! Debug console and legacy byte I/O.
//!
//! # References
//!
//! - Specification: [RISC-V SBI DBCN extension](https://docs.riscv.org/reference/sbi/v3.0/ext-debug-console.html) —
//!   console transfers, error handling, and the legacy console replacement.

#![forbid(unsafe_code)]

use alloc::boxed::Box;
use core::fmt;
use runtime::memory::{PhysAddr, PhysAddrRange, SupervisorMemory};
use rustsbi::{Console, Physical, SbiRet};
use spin::Mutex;

use crate::driver::ConsoleDevice;

/// Bounds stack use and work performed while the console device lock is held.
const MAX_TRANSFER_BYTES: usize = 256;

/// SBI console service over a platform console device.
pub(crate) struct SbiConsole {
    device: &'static Mutex<Box<dyn ConsoleDevice>>,
    supervisor_memory: &'static SupervisorMemory,
}

impl SbiConsole {
    /// Creates a new SBI console handle over the published console device.
    #[inline]
    pub(crate) fn new(
        device: &'static Mutex<Box<dyn ConsoleDevice>>,
        supervisor_memory: &'static SupervisorMemory,
    ) -> Self {
        Self {
            device,
            supervisor_memory,
        }
    }

    // Console transfers require the complete buffer to belong to registered
    // supervisor memory. A zero-length transfer does not inspect its address.
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
            return Err(SbiRet::failed());
        }

        let range =
            PhysAddrRange::from_start_len(start, len).map_err(|_| SbiRet::invalid_address())?;
        if self.supervisor_memory.check_range(range).is_err() {
            return Err(SbiRet::failed());
        }

        Ok((start, len))
    }

    pub(super) fn write_byte_blocking(&self, byte: u8) {
        while self.device.lock().write(&[byte]) == 0 {
            core::hint::spin_loop();
        }
    }

    pub(super) fn try_read_byte(&self) -> Option<u8> {
        let mut byte = 0;
        (self.device.lock().read(core::slice::from_mut(&mut byte)) == 1).then_some(byte)
    }
}

impl Console for SbiConsole {
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
            return SbiRet::failed();
        }
        let count = self.device.lock().write(&chunk[..chunk_len]);
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
        let count = self.device.lock().read(&mut chunk[..chunk_len]);
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
            return SbiRet::failed();
        }
        SbiRet::success(count)
    }

    /// Writes `byte` to the console.
    #[inline]
    fn write_byte(&self, byte: u8) -> SbiRet {
        self.write_byte_blocking(byte);
        SbiRet::success(0)
    }
}

/// Prints formatted arguments to the console device, if one is present.
///
/// The `print!`/`println!` macros route here. Writes proceed until the
/// device reports progress; a zero-byte write is an error.
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write as _;

    struct ConsoleWriter<'a> {
        device: &'a Mutex<Box<dyn ConsoleDevice>>,
    }

    impl fmt::Write for ConsoleWriter<'_> {
        #[inline]
        fn write_str(&mut self, s: &str) -> fmt::Result {
            let mut bytes = s.as_bytes();
            while !bytes.is_empty() {
                let count = self.device.lock().write(bytes);
                if count == 0 {
                    return Err(fmt::Error);
                }
                bytes = &bytes[count..];
            }
            Ok(())
        }
    }

    if let Some(device) = crate::platform::console_device() {
        ConsoleWriter { device }.write_fmt(args).unwrap();
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
