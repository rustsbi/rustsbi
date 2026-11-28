//! RV64 pointer-masking support probes and FWFT configuration.

use csr::{
    Read, ReadMode, Write,
    standard::isa::{privileged::machine::Menvcfg, values::pointer_masking::PointerMasking},
};

use crate::{Error, Result};

fn restore(current: Menvcfg<ReadMode>) -> Result<()> {
    // SAFETY: FWFT owns this hart's configuration with trap recovery ready and IRQs excluded.
    let restored = unsafe {
        Menvcfg::try_write(current.into_write()).map_err(|_| Error::IoError)?;
        Menvcfg::try_read().map_err(|_| Error::IoError)?
    };
    if restored.get(Menvcfg::PMM) == current.get(Menvcfg::PMM) {
        Ok(())
    } else {
        Err(Error::IoError)
    }
}

fn supported(current: Menvcfg<ReadMode>) -> Result<bool> {
    for candidate in [PointerMasking::Bits7, PointerMasking::Bits16] {
        let mut reg = current.into_write();
        reg.set(Menvcfg::PMM, candidate);
        // SAFETY: FWFT get/set establish recovery and exclude local interrupts.
        let read_back = unsafe {
            if Menvcfg::try_write(reg).is_err() {
                continue;
            }
            Menvcfg::try_read()
                .ok()
                .and_then(|value| value.get(Menvcfg::PMM).ok())
        };
        restore(current)?;
        if matches!(
            read_back,
            Some(PointerMasking::Bits7 | PointerMasking::Bits16)
        ) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn access(requested: Option<usize>) -> Result<usize> {
    let requested = requested
        .map(|value| PointerMasking::from_pmlen(value).ok_or(Error::InvalidArgs))
        .transpose()?;
    // SAFETY: FWFT establishes recovery and excludes local interrupts.
    let current = unsafe { Menvcfg::try_read() }.map_err(|_| Error::NotSupported)?;
    let mode = current.get(Menvcfg::PMM);
    let target = match requested {
        Some(target) => target,
        None => mode.map_err(|_| Error::NotSupported)?,
    };
    if mode == Ok(target) {
        if target == PointerMasking::Disabled && !supported(current)? {
            return Err(Error::NotSupported);
        }
        return Ok(target.pmlen());
    }
    let mut reg = current.into_write();
    reg.set(Menvcfg::PMM, target);
    // SAFETY: FWFT excludes interrupts and MPRV is clear while changing S-mode pointer masking.
    let accepted = unsafe {
        Menvcfg::try_write(reg).map_err(|_| Error::NotSupported)?;
        Menvcfg::try_read().is_ok_and(|reg| reg.get(Menvcfg::PMM) == Ok(target))
    };
    if !accepted {
        restore(current)?;
        return Err(Error::NotSupported);
    }
    Ok(target.pmlen())
}
