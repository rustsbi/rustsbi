//! Serialized firmware console capability.

use alloc::boxed::Box;
use core::fmt::{self, Write};

use spin::{Mutex, MutexGuard};

pub(crate) trait ConsoleDevice: Send {
    fn read(&mut self, destination: &mut [u8]) -> Result<usize, ConsoleError>;
    fn write(&mut self, source: &[u8]) -> Result<usize, ConsoleError>;
}

/// Failure from a physical console operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsoleError {
    /// The bound physical console reported a permanent failure.
    Failed,
}

struct ConsoleState {
    device: Mutex<Box<dyn ConsoleDevice>>,
}

/// Shared serialized access to the one bound firmware console.
///
/// Cloning shares access authority to the same stable state. It never copies,
/// reconstructs, or independently initializes the physical device.
#[derive(Clone)]
pub struct Console {
    state: &'static ConsoleState,
}

impl Console {
    pub(crate) fn new(device: Box<dyn ConsoleDevice>) -> Self {
        let state = Box::leak(Box::new(ConsoleState {
            device: Mutex::new(device),
        }));
        Self { state }
    }

    /// Performs one non-blocking read and returns the number of bytes read.
    pub fn read(&self, destination: &mut [u8]) -> Result<usize, ConsoleError> {
        self.state.device.lock().read(destination)
    }

    /// Performs one non-blocking write and returns the number of bytes written.
    pub fn write(&self, source: &[u8]) -> Result<usize, ConsoleError> {
        self.state.device.lock().write(source)
    }

    /// Writes one complete byte while holding the serialization boundary.
    pub fn write_byte(&self, byte: u8) -> Result<(), ConsoleError> {
        let mut device = self.state.device.lock();
        write_all(&mut device, core::slice::from_ref(&byte))
    }

    /// Formats and writes one complete record under one console lock.
    pub fn write_fmt(&self, arguments: fmt::Arguments<'_>) -> Result<(), ConsoleError> {
        let device = self.state.device.lock();
        let mut writer = LockedWriter {
            device,
            error: None,
        };
        if writer.write_fmt(arguments).is_err() {
            return Err(writer.error.unwrap_or(ConsoleError::Failed));
        }
        Ok(())
    }

    /// Attempts one non-blocking write without waiting for another owner.
    ///
    /// A zero count means either that the console was busy or that the device
    /// accepted no bytes. Panic reporting intentionally does not distinguish
    /// those cases and never steals the lock.
    pub fn try_write(&self, source: &[u8]) -> Result<usize, ConsoleError> {
        let Some(mut device) = self.state.device.try_lock() else {
            return Ok(0);
        };
        device.write(source)
    }

    /// Attempts one bounded formatted record under exactly one non-waiting
    /// console acquisition.
    ///
    /// A busy console silently drops the record. Once acquired, formatting
    /// stops at the first device call that makes no progress; it never spins.
    pub fn try_write_fmt(&self, arguments: fmt::Arguments<'_>) -> Result<(), ConsoleError> {
        let Some(device) = self.state.device.try_lock() else {
            return Ok(());
        };
        let mut writer = TryWriter {
            device,
            error: None,
        };
        if writer.write_fmt(arguments).is_err() {
            return Err(writer.error.unwrap_or(ConsoleError::Failed));
        }
        Ok(())
    }
}

#[crate::mtest]
fn bound_mmio_accepts_a_complete_record() {
    let console = crate::test_support::console().expect("test console must be initialized");
    assert!(
        console
            .write_fmt(format_args!("@@RUSTSBI_MTEST type=CONSOLE_PROBE\n"))
            .is_ok()
    );
}

fn write_all(
    device: &mut MutexGuard<'_, Box<dyn ConsoleDevice>>,
    mut source: &[u8],
) -> Result<(), ConsoleError> {
    while !source.is_empty() {
        let written = device.write(source)?;
        if written == 0 {
            core::hint::spin_loop();
            continue;
        }
        source = source.get(written..).ok_or(ConsoleError::Failed)?;
    }
    Ok(())
}

struct LockedWriter<'lock> {
    device: MutexGuard<'lock, Box<dyn ConsoleDevice>>,
    error: Option<ConsoleError>,
}

impl Write for LockedWriter<'_> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if let Err(error) = write_all(&mut self.device, value.as_bytes()) {
            self.error = Some(error);
            return Err(fmt::Error);
        }
        Ok(())
    }
}

struct TryWriter<'lock> {
    device: MutexGuard<'lock, Box<dyn ConsoleDevice>>,
    error: Option<ConsoleError>,
}

impl Write for TryWriter<'_> {
    fn write_str(&mut self, mut value: &str) -> fmt::Result {
        while !value.is_empty() {
            match self.device.write(value.as_bytes()) {
                Ok(0) => return Err(fmt::Error),
                Ok(written) => {
                    value = value.get(written..).ok_or(fmt::Error)?;
                }
                Err(error) => {
                    self.error = Some(error);
                    return Err(fmt::Error);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicUsize, Ordering};

    struct RecordingDevice {
        output: Arc<Mutex<alloc::vec::Vec<u8>>>,
        chunk: usize,
    }

    impl ConsoleDevice for RecordingDevice {
        fn read(&mut self, _: &mut [u8]) -> Result<usize, ConsoleError> {
            Ok(0)
        }

        fn write(&mut self, source: &[u8]) -> Result<usize, ConsoleError> {
            let count = source.len().min(self.chunk);
            self.output.lock().extend_from_slice(&source[..count]);
            Ok(count)
        }
    }

    fn recording_console(chunk: usize) -> (Console, Arc<Mutex<alloc::vec::Vec<u8>>>) {
        let output = Arc::new(Mutex::new(alloc::vec::Vec::new()));
        let console = Console {
            state: Box::leak(Box::new(ConsoleState {
                device: Mutex::new(Box::new(RecordingDevice {
                    output: output.clone(),
                    chunk,
                })),
            })),
        };
        (console, output)
    }

    #[test]
    fn one_formatted_record_holds_the_lock_across_partial_writes() {
        let (console, output) = recording_console(2);
        console.write_fmt(format_args!("value={}", 17)).unwrap();
        assert_eq!(&*output.lock(), b"value=17");
    }

    #[test]
    fn panic_record_completes_partial_writes_under_one_try_lock() {
        let (console, output) = recording_console(2);
        console.try_write_fmt(format_args!("panic {}", 17)).unwrap();
        assert_eq!(&*output.lock(), b"panic 17");
    }

    #[test]
    fn busy_console_drops_a_panic_record_without_stealing_the_lock() {
        let (console, output) = recording_console(2);
        let owner = console.state.device.lock();
        assert_eq!(console.try_write_fmt(format_args!("panic")), Ok(()));
        assert!(output.lock().is_empty());
        drop(owner);
    }

    #[test]
    fn zero_progress_stops_a_panic_record_after_one_device_call() {
        struct ZeroProgressDevice(Arc<AtomicUsize>);

        impl ConsoleDevice for ZeroProgressDevice {
            fn read(&mut self, _: &mut [u8]) -> Result<usize, ConsoleError> {
                Ok(0)
            }

            fn write(&mut self, _: &[u8]) -> Result<usize, ConsoleError> {
                self.0.fetch_add(1, Ordering::Relaxed);
                Ok(0)
            }
        }

        let calls = Arc::new(AtomicUsize::new(0));
        let console = Console {
            state: Box::leak(Box::new(ConsoleState {
                device: Mutex::new(Box::new(ZeroProgressDevice(calls.clone()))),
            })),
        };
        assert_eq!(
            console.try_write_fmt(format_args!("panic")),
            Err(ConsoleError::Failed)
        );
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }
}
