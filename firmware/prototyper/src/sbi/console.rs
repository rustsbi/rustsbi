use alloc::boxed::Box;
use core::fmt;
use rustsbi::{Console, Physical, SbiRet};
use spin::{Mutex, Once};

use crate::platform::BoardInfo;
use crate::platform::console::MachineConsoleType;
use crate::platform::console::Uart16550Wrap;
use crate::platform::console::UartAxiLiteWrap;
use crate::platform::console::UartBflbWrap;
use crate::platform::console::UartPl011Wrap;
use crate::platform::console::UartSifiveWrap;
use crate::platform::console::UartXscaleWrap;
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
pub trait ConsoleDevice: Send {
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

/// The machine console device, published once by [`init`] when the board
/// provides a console.
///
/// Invariant: published (pre-`READY`) before the logger is brought up, so
/// that early boot messages are printable; read afterwards by the print
/// macros and the console ecall paths. Before publication, [`_print`] is a
/// no-op (matching the previous `have_console` skip).
static CONSOLE_DEVICE: Once<Mutex<Box<dyn ConsoleDevice>>> = Once::new();

/// An implementation of the SBI console interface that wraps a console device.
///
/// This provides a safe interface for interacting with console hardware through the
/// SBI specification. The handle borrows the device published in
/// [`CONSOLE_DEVICE`]; all I/O goes through the device lock.
pub struct SbiConsole {
    inner: &'static Mutex<Box<dyn ConsoleDevice>>,
}

impl SbiConsole {
    /// Creates a new SBI console handle over the published console device.
    ///
    /// # Arguments
    /// * `inner` - The published console device, protected by a mutex
    #[inline]
    pub fn new(inner: &'static Mutex<Box<dyn ConsoleDevice>>) -> Self {
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
    pub fn putchar(&self, c: usize) -> usize {
        let byte = [c as u8];
        while self.inner.lock().write(&byte) == 0 {
            core::hint::spin_loop();
        }
        0
    }

    /// Reads a single character from the console.
    ///
    /// # Returns
    /// Returns the character as a usize on success, or '-1' for failure.
    #[inline]
    pub fn getchar(&self) -> usize {
        let mut c = 0u8;
        let nread = self.inner.lock().read(core::slice::from_mut(&mut c));
        if nread == 1 { c as usize } else { usize::MAX }
    }

    // Rejects buffers that this firmware cannot safely turn into raw slices.
    //
    // The SBI address tuple may still be valid,
    // but this implementation only accepts buffers inside `board_info().memory_range`.
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

/// Prints formatted arguments to the console device, if one is present.
///
/// The `print!`/`println!` macros route here; before the console device is
/// published this is a no-op (matching the previous `have_console` skip).
/// The chunked-write loop keeps the write semantics of the old
/// `fmt::Write for SbiConsole` impl: write until the device reports
/// progress, error on a zero-byte write.
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

/// Global function to write a character to the console.
#[inline]
pub fn putchar(c: usize) -> usize {
    SbiConsole::new(
        CONSOLE_DEVICE
            .get()
            .expect("console device not initialized"),
    )
    .putchar(c)
}

/// Global function to read a character from the console.
#[inline]
pub fn getchar() -> usize {
    SbiConsole::new(
        CONSOLE_DEVICE
            .get()
            .expect("console device not initialized"),
    )
    .getchar()
}

/// Initializes the SBI console from the discovered board info, then brings up
/// the logger (bundled, so that early boot messages are printable).
pub(crate) fn init(board: &BoardInfo) -> Option<SbiConsole> {
    // init console and logger
    let console = board.console.map(|(base, console_type)| {
        let device: Box<dyn ConsoleDevice> = match console_type {
            MachineConsoleType::Uart16550U8 => Box::new(Uart16550Wrap::<u8>::new(base)),
            MachineConsoleType::Uart16550U32 => Box::new(Uart16550Wrap::<u32>::new(base)),
            MachineConsoleType::UartAxiLite => Box::new(UartAxiLiteWrap::new(base)),
            MachineConsoleType::UartBflb => Box::new(UartBflbWrap::new(base)),
            MachineConsoleType::UartSifive => Box::new(UartSifiveWrap::new(base)),
            MachineConsoleType::UartPl011 => Box::new(UartPl011Wrap::new(base)),
            MachineConsoleType::UartXscale => {
                Box::new(UartXscaleWrap::new(base, board.console_clock))
            }
        };
        CONSOLE_DEVICE.call_once(|| Mutex::new(device));
        SbiConsole::new(CONSOLE_DEVICE.get().expect("console device just published"))
    });
    logger::Logger::init().unwrap();
    info!("Hello RustSBI!");
    console
}
