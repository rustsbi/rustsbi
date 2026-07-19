//! Debug-console protocol adapter.

use machine::{Console, ConsoleError, SModeMemory, SModeMemoryError};
use rustsbi::{Physical, SbiRet};

const TRANSFER_CHUNK: usize = 64;

pub(super) struct DebugConsole {
    console: Console,
    memory: SModeMemory,
}

impl DebugConsole {
    pub(super) fn new(console: Console, memory: SModeMemory) -> Self {
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
            Err(error) => return memory_error(error),
        };
        let mut total = 0usize;
        let mut buffer = [0u8; TRANSFER_CHUNK];
        while total < bytes.num_bytes() {
            let count = (bytes.num_bytes() - total).min(buffer.len());
            if let Err(error) = source.read(&mut buffer[..count]) {
                return memory_error(error);
            }
            match self.console.write(&buffer[..count]) {
                Ok(0) => break,
                Ok(written) if written <= count => {
                    total += written;
                    if written != count {
                        break;
                    }
                }
                Ok(_) | Err(ConsoleError::Failed) => return SbiRet::failed(),
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
            Err(error) => return memory_error(error),
        };
        let mut total = 0usize;
        let mut buffer = [0u8; TRANSFER_CHUNK];
        while total < bytes.num_bytes() {
            let capacity = (bytes.num_bytes() - total).min(buffer.len());
            let count = match self.console.read(&mut buffer[..capacity]) {
                Ok(count) if count <= capacity => count,
                Ok(_) | Err(ConsoleError::Failed) => return SbiRet::failed(),
            };
            if count == 0 {
                break;
            }
            if let Err(error) = destination.write(&buffer[..count]) {
                return memory_error(error);
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
            Err(ConsoleError::Failed) => SbiRet::failed(),
        }
    }
}

fn memory_error(error: SModeMemoryError) -> SbiRet {
    match error {
        SModeMemoryError::InvalidRange | SModeMemoryError::UnsupportedRange => {
            SbiRet::invalid_param()
        }
        SModeMemoryError::Fault => SbiRet::failed(),
    }
}
