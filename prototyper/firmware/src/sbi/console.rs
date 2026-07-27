//! Debug-console protocol adapter.

use machine::memory::{MemoryError, SupervisorMemory};
use machine::{Console, ConsoleError};
use rustsbi::{Physical, SbiRet};
use sbi_spec::binary::Error;

const TRANSFER_CHUNK: usize = 64;

pub(super) struct DebugConsole {
    console: Console,
    memory: SupervisorMemory,
}

impl DebugConsole {
    pub(super) fn new(console: Console, memory: SupervisorMemory) -> Self {
        Self { console, memory }
    }
}

impl rustsbi::Console for DebugConsole {
    fn write(&self, bytes: Physical<&[u8]>) -> SbiRet {
        let mut source = match self.memory.reader(
            bytes.phys_addr_lo(),
            bytes.phys_addr_hi(),
            bytes.num_bytes(),
        ) {
            Ok(source) => source,
            Err(error) => return memory_error(error).into(),
        };
        let mut total = 0usize;
        let mut buffer = [0u8; TRANSFER_CHUNK];
        while total < bytes.num_bytes() {
            let count = (bytes.num_bytes() - total).min(buffer.len());
            if let Err(error) = source.read_exact(&mut buffer[..count]) {
                return memory_error(error).into();
            }
            match self.console.write(&buffer[..count]) {
                Ok(0) => break,
                Ok(written) if written <= count => {
                    total += written;
                    if written != count {
                        break;
                    }
                }
                Ok(_) | Err(ConsoleError::Failed) => return Error::Failed.into(),
            }
        }
        SbiRet::success(total)
    }

    fn read(&self, bytes: Physical<&mut [u8]>) -> SbiRet {
        let mut destination = match self.memory.writer(
            bytes.phys_addr_lo(),
            bytes.phys_addr_hi(),
            bytes.num_bytes(),
        ) {
            Ok(destination) => destination,
            Err(error) => return memory_error(error).into(),
        };
        let mut total = 0usize;
        let mut buffer = [0u8; TRANSFER_CHUNK];
        while total < bytes.num_bytes() {
            let capacity = (bytes.num_bytes() - total).min(buffer.len());
            let count = match self.console.read(&mut buffer[..capacity]) {
                Ok(count) if count <= capacity => count,
                Ok(_) | Err(ConsoleError::Failed) => return Error::Failed.into(),
            };
            if count == 0 {
                break;
            }
            if let Err(error) = destination.write_all(&buffer[..count]) {
                return memory_error(error).into();
            }
            total += count;
            if count != capacity {
                break;
            }
        }
        SbiRet::success(total)
    }

    fn write_byte(&self, byte: u8) -> SbiRet {
        match self.console.write_byte(byte) {
            Ok(()) => SbiRet::success(0),
            Err(ConsoleError::Failed) => Error::Failed.into(),
        }
    }
}

fn memory_error(error: MemoryError) -> Error {
    match error {
        MemoryError::InvalidRange | MemoryError::UnsupportedRange => Error::InvalidParam,
        MemoryError::Fault => Error::Failed,
    }
}
