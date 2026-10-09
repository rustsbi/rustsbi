//! Bounded debug-trigger discovery with selector restoration.

use crate::csr::{Readable, Tdata1, Tselect, Writable};
use crate::trap::Error;

const MAX_TRIGGERS: usize = 256;

pub(super) fn probe_count() -> Result<usize, Error> {
    riscv::interrupt::machine::free(|| {
        let previous = match Tselect::read() {
            Ok(value) => value,
            Err(Error::UnsupportedInstruction) => return Ok(0),
            Err(error) => return Err(error),
        };
        let result = probe_selected();
        // Attempt restoration even after a failed probe; a restore error takes precedence.
        Tselect::write(previous)?;
        result
    })
}

fn probe_selected() -> Result<usize, Error> {
    let mut count = 0;
    for index in 0..MAX_TRIGGERS {
        match Tselect::write(index) {
            Ok(()) => {}
            Err(Error::UnsupportedInstruction) => break,
            Err(error) => return Err(error),
        }
        if Tselect::read()? != index {
            break;
        }
        match Tdata1::read() {
            Ok(data) if data.trigger_type() != 0 => count += 1,
            Ok(_) => {}
            Err(Error::UnsupportedInstruction) => break,
            Err(error) => return Err(error),
        }
    }
    Ok(count)
}
