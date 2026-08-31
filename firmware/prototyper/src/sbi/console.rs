use alloc::boxed::Box;
use core::fmt;
use rustsbi::{Console, Physical, SbiRet};
use spin::{Mutex, Once};

use crate::driver::ConsoleDevice;
use crate::platform::BoardInfo;
use crate::sbi::logger;

// Checks whether `(phys_addr_lo, phys_addr_hi, len)` can be represented
// as a native address range on this machine.
//
// See the shared-memory physical address rules in
// <https://github.com/riscv-non-isa/riscv-sbi-doc/blob/v3.0/src/binary-encoding.adoc>
// and the DBCN call definitions in
// <https://github.com/riscv-non-isa/riscv-sbi-doc/blob/v3.0/src/ext-debug-console.adoc>.
#[inline]
fn checked_physical_addr(lo: usize, hi: usize, len: usize) -> Result<usize, SbiRet> {
    // If the address exceeds the native `usize` range, the SBI spec allows
    // returning an error rather than truncating it.
    if hi != 0 {
        return Err(SbiRet::failed());
    }

    // A `lo + len` overflow cannot be a valid buffer on this XLEN.
    let _end = lo.checked_add(len).ok_or_else(SbiRet::invalid_address)?;

    Ok(lo)
}

/// The machine console device, published once by [`init`] when the board
/// provides a console.
///
/// Invariant: published (pre-`READY`) before the logger is brought up, so
/// that early boot messages are printable; read afterwards by the print
/// macros and the console ecall paths. Before publication, [`_print`] is a
/// no-op.
static CONSOLE_DEVICE: Once<Mutex<Box<dyn ConsoleDevice>>> = Once::new();

/// SBI console service over the device published in [`CONSOLE_DEVICE`];
/// all I/O goes through the device lock.
pub struct SbiConsole {
    inner: &'static Mutex<Box<dyn ConsoleDevice>>,
}

impl SbiConsole {
    /// Creates a new SBI console handle over the published console device.
    #[inline]
    pub fn new(inner: &'static Mutex<Box<dyn ConsoleDevice>>) -> Self {
        Self { inner }
    }

    /// Writes a single character; returns 0 for success per the legacy
    /// SBI console putchar convention.
    #[inline]
    pub fn putchar(&self, c: usize) -> usize {
        let byte = [c as u8];
        while self.inner.lock().write(&byte) == 0 {
            core::hint::spin_loop();
        }
        0
    }

    /// Reads a single character, returning `usize::MAX` when the console
    /// has no input (the legacy getchar failure value).
    #[inline]
    pub fn getchar(&self) -> usize {
        let mut c = 0u8;
        let nread = self.inner.lock().read(core::slice::from_mut(&mut c));
        if nread == 1 { c as usize } else { usize::MAX }
    }

    // Only buffers inside the discovered memory range can be safely turned
    // into raw slices; the SBI address tuple itself may be valid.
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

        match crate::platform::board_info().memory_range.as_ref() {
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

        // SAFETY: `checked_physical_buffer` validated the range as inside
        // discovered RAM, so byte reads stay in bounds and initialized.
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

        // SAFETY: `checked_physical_buffer` validated the range as inside
        // discovered RAM, so byte writes stay in bounds.
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

    if let Some(device) = CONSOLE_DEVICE.get() {
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

/// Writes a character through the published console device.
///
/// # Panics
///
/// Panics if no console device was published.
#[inline]
pub fn putchar(c: usize) -> usize {
    SbiConsole::new(
        CONSOLE_DEVICE
            .get()
            .expect("console device not initialized"),
    )
    .putchar(c)
}

/// Reads a character through the published console device.
///
/// # Panics
///
/// Panics if no console device was published.
#[inline]
pub fn getchar() -> usize {
    SbiConsole::new(
        CONSOLE_DEVICE
            .get()
            .expect("console device not initialized"),
    )
    .getchar()
}

/// Publishes the discovered console device, initializes the logger, and
/// prints the boot banner.
pub(crate) fn init(board: &BoardInfo) -> Option<SbiConsole> {
    let console = crate::driver::console_device(board).map(|device| {
        CONSOLE_DEVICE.call_once(|| Mutex::new(device));
        SbiConsole::new(CONSOLE_DEVICE.get().expect("console device just published"))
    });
    logger::Logger::init().unwrap();
    info!("Hello RustSBI!");
    console
}
