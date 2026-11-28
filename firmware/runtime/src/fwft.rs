//! Firmware feature configuration used by the SBI FWFT service.

use csr::{
    Clear, Read, Set, Write,
    standard::isa::{
        privileged::machine::{self, Menvcfg, MenvcfgBit, Misa},
        values::trap::{Exception, ExceptionSet},
    },
};

#[cfg(target_pointer_width = "64")]
mod pointer_masking;

use crate::rustsbi::spec::fwft::feature_type;
use crate::{Error, Result};

/// Sets a local SBI firmware feature and verifies the requested value by readback.
///
/// [`Error::IoError`] means that restoring the previous configuration failed.
pub fn set(feature_id: u32, value: usize) -> Result<()> {
    access(feature_id, Some(value)).map(|_| ())
}

/// Reads one supported firmware feature on the current hart.
///
/// A disabled WARL field can require a trial write to distinguish it from a
/// read-only zero. A failed restoration returns [`Error::IoError`].
pub fn get(feature_id: u32) -> Result<usize> {
    access(feature_id, None)
}

fn access(feature_id: u32, requested: Option<usize>) -> Result<usize> {
    if !crate::recovery::is_ready() {
        return Err(Error::NotReady);
    }
    let feature_id = feature_id as usize;
    if feature_id == feature_type::MISALIGNED_EXC_DELEG {
        if !Misa::read().has_extension('S') {
            return Err(Error::NotSupported);
        }
        return match requested {
            None => misaligned_delegated(machine::read_medeleg_word()).map(usize::from),
            Some(value @ 0..=1) => crate::irq::with_local_disabled(|_| {
                set_misaligned_delegation(value == 1)?;
                Ok(value)
            }),
            Some(_) => Err(Error::InvalidArgs),
        };
    }
    crate::irq::with_local_disabled(|_| match feature_id {
        feature_type::LANDING_PAD => env_bit(Menvcfg::LPE, requested),
        feature_type::SHADOW_STACK => env_bit(Menvcfg::SSE, requested),
        feature_type::DOUBLE_TRAP => env_bit(Menvcfg::DTE, requested),
        feature_type::PTE_AD_HW_UPDATING => env_bit(Menvcfg::ADUE, requested),
        #[cfg(target_pointer_width = "64")]
        feature_type::POINTER_MASKING_PMLEN => pointer_masking::access(requested),
        id if id > feature_type::POINTER_MASKING_PMLEN => Err(Error::AccessDenied),
        _ => Err(Error::NotSupported),
    })
}

fn misaligned_delegated(exceptions: ExceptionSet) -> Result<bool> {
    let load = exceptions.contains(Exception::LoadMisaligned);
    let store = exceptions.contains(Exception::StoreMisaligned);
    (load == store).then_some(load).ok_or(Error::NotSupported)
}

fn set_misaligned_delegation(enabled: bool) -> Result<()> {
    let saved = machine::read_medeleg_word();
    if misaligned_delegated(saved) == Ok(enabled) {
        return Ok(());
    }
    let mut requested = saved;
    requested.set(Exception::LoadMisaligned, enabled);
    requested.set(Exception::StoreMisaligned, enabled);
    // SAFETY: Local IRQs are masked and only the two misaligned exceptions change.
    unsafe { machine::write_medeleg_word(requested) };
    if misaligned_delegated(machine::read_medeleg_word()) == Ok(enabled) {
        return Ok(());
    }
    // SAFETY: The same exclusion covers restoration of the original routing.
    unsafe { machine::write_medeleg_word(saved) };
    if machine::read_medeleg_word() == saved {
        Err(Error::NotSupported)
    } else {
        Err(Error::IoError)
    }
}

fn env_bit<const BIT: u32>(field: MenvcfgBit<BIT>, requested: Option<usize>) -> Result<usize> {
    if let Some(2..) = requested {
        return Err(Error::InvalidArgs);
    }
    // SAFETY: FWFT owns the current hart's field with recovery ready and IRQs excluded.
    let read = || unsafe { machine::try_read_menvcfg_bit(field) };
    // SAFETY: The same excluded context owns each atomic update of this field.
    let write = |enabled| unsafe {
        if enabled {
            Menvcfg::try_write(Set(field))
        } else {
            Menvcfg::try_write(Clear(field))
        }
    };
    let current = read().map_err(|_| Error::NotSupported)?;
    let target = requested.map_or(current, |value| value == 1);
    if current && target {
        return Ok(1);
    }

    // A clear bit needs a temporary enable to distinguish a disabled feature from WARL zero.
    let probe = !current && !target;
    let trial = !current;
    write(trial).map_err(|_| Error::NotSupported)?;
    let accepted = read() == Ok(trial);
    let restore_failed =
        (probe || !accepted) && write(current).and_then(|()| read()) != Ok(current);
    if field.mask() == Menvcfg::ADUE.mask() {
        // SAFETY: The FWFT entry checked recovery and holds interrupts masked
        // while a change to ADUE is made visible to subsequent translations.
        unsafe { crate::fence::synchronize_translations() };
    }
    if restore_failed {
        Err(Error::IoError)
    } else if accepted {
        Ok(usize::from(target))
    } else {
        Err(Error::NotSupported)
    }
}
