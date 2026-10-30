//! Firmware feature configuration used by the SBI FWFT service.

use crate::rustsbi::spec::fwft::feature_type;
use crate::{
    Error, Result,
    csr::{csrr, csrw},
};
use csr::{machine, values::trap::Exception};

/// Sets one supported firmware feature on the current hart.
pub fn set(feature_id: u32, value: usize) -> Result<()> {
    if !crate::boot::traps_ready() {
        return Err(Error::NotReady);
    }
    crate::irq::with_local_disabled(|_| set_feature(feature_id, value))
}

/// Reads one supported firmware feature on the current hart.
/// WARL probes restore the original configuration when access succeeds.
pub fn get(feature_id: u32) -> Result<usize> {
    if !crate::boot::traps_ready() {
        return Err(Error::NotReady);
    }
    if feature_id as usize == feature_type::MISALIGNED_EXC_DELEG {
        return get_feature(feature_id);
    }
    // The remaining supported queries temporarily write WARL configuration.
    crate::irq::with_local_disabled(|_| get_feature(feature_id))
}

// `menvcfg` fields defined by the corresponding RISC-V extensions.
const ENVCFG_LPE: usize = 1 << 0; // Landing pad (Zicfilp)
const ENVCFG_DTE: usize = 1 << 1; // Double trap (Smdbltrp)
const ENVCFG_ADUE: usize = 1 << 5; // PTE A/D hardware updating (SVADU)
const ENVCFG_SSE: usize = 1 << 8; // Shadow stack (Zicfiss)
const ENVCFG_PMM_SHIFT: usize = 9; // Pointer masking tag length (Smnpm)
const ENVCFG_PMM: usize = 0b11 << ENVCFG_PMM_SHIFT;

fn misaligned_delegated() -> bool {
    let exceptions = machine::ExceptionDelegation::read();
    exceptions.contains(Exception::LoadMisaligned)
        || exceptions.contains(Exception::StoreMisaligned)
}

fn set_misaligned_delegation(enabled: bool) {
    const MISALIGNED: usize = (1 << 4) | (1 << 6);
    let current = usize::from(machine::ExceptionDelegation::read());
    let next = if enabled {
        current | MISALIGNED
    } else {
        current & !MISALIGNED
    };
    // SAFETY: Local IRQ exclusion protects this hart's delegation update.
    unsafe { machine::ExceptionDelegation::write(next.into()) };
}

fn has_s_mode() -> bool {
    machine::Isa::read().has_extension('S')
}

fn menvcfg_read() -> Option<usize> {
    // SAFETY: Runtime entry is installed; menvcfg reads have no side effects.
    csrr!(machine::EnvironmentConfig).ok()
}

fn menvcfg_write(value: usize) -> bool {
    // SAFETY: Callers change only the requested lower-mode feature bits,
    // preserving the M-mode execution and translation context.
    csrw!(machine::EnvironmentConfig, value).is_ok()
}

fn menvcfg_bit(feature_id: usize) -> Option<usize> {
    match feature_id {
        feature_type::LANDING_PAD => Some(ENVCFG_LPE),
        feature_type::SHADOW_STACK => Some(ENVCFG_SSE),
        feature_type::DOUBLE_TRAP => Some(ENVCFG_DTE),
        feature_type::PTE_AD_HW_UPDATING => Some(ENVCFG_ADUE),
        _ => None,
    }
}

fn set_menvcfg_bit(bit: usize, value: usize) -> Result<()> {
    if value > 1 {
        return Err(Error::InvalidArgs);
    }
    let Some(current) = menvcfg_read() else {
        return Err(Error::NotSupported);
    };
    let next = if value == 1 {
        current | bit
    } else {
        current & !bit
    };
    if !menvcfg_write(next) {
        return Err(Error::NotSupported);
    }
    let Some(read_back) = menvcfg_read() else {
        return Err(Error::NotSupported);
    };
    // WARL fields may ignore writes when the backing extension is absent.
    if (read_back & bit) != (next & bit) {
        return Err(Error::NotSupported);
    }
    Ok(())
}

fn set_pmm(value: usize) -> Result<()> {
    if value > 3 {
        return Err(Error::InvalidArgs);
    }
    let Some(current) = menvcfg_read() else {
        return Err(Error::NotSupported);
    };
    let next = (current & !ENVCFG_PMM) | (value << ENVCFG_PMM_SHIFT);
    if !menvcfg_write(next) {
        return Err(Error::NotSupported);
    }
    let Some(read_back) = menvcfg_read() else {
        return Err(Error::NotSupported);
    };
    // PMM is WARL and may reject tag lengths unsupported by Smnpm.
    if (read_back & ENVCFG_PMM) != (next & ENVCFG_PMM) {
        return Err(Error::NotSupported);
    }
    Ok(())
}

// Probe whether WARL fields retain set bits, then attempt to restore
// the original value.
fn menvcfg_bits_supported(mask: usize) -> bool {
    let Some(current) = menvcfg_read() else {
        return false;
    };
    if !menvcfg_write(current | mask) {
        return false;
    }
    let Some(probed) = menvcfg_read() else {
        let _ = menvcfg_write(current);
        return false;
    };
    let _ = menvcfg_write(current);
    (probed & mask) != 0
}
fn set_feature(feature_id: u32, value: usize) -> Result<()> {
    match feature_id as usize {
        feature_type::MISALIGNED_EXC_DELEG => {
            if !has_s_mode() {
                return Err(Error::NotSupported);
            }
            if value > 1 {
                return Err(Error::InvalidArgs);
            }
            set_misaligned_delegation(value == 1);
            Ok(())
        }
        feature_type::POINTER_MASKING_PMLEN => set_pmm(value),
        _ => match menvcfg_bit(feature_id as usize) {
            Some(bit) => set_menvcfg_bit(bit, value),
            None => Err(Error::NotSupported),
        },
    }
}

fn get_feature(feature_id: u32) -> Result<usize> {
    match feature_id as usize {
        feature_type::MISALIGNED_EXC_DELEG => {
            if !has_s_mode() {
                return Err(Error::NotSupported);
            }
            Ok(misaligned_delegated() as usize)
        }
        feature_type::POINTER_MASKING_PMLEN => {
            if !menvcfg_bits_supported(ENVCFG_PMM) {
                return Err(Error::NotSupported);
            }
            match menvcfg_read() {
                Some(value) => Ok((value & ENVCFG_PMM) >> ENVCFG_PMM_SHIFT),
                None => Err(Error::NotSupported),
            }
        }
        _ => match menvcfg_bit(feature_id as usize) {
            Some(bit) => {
                if !menvcfg_bits_supported(bit) {
                    return Err(Error::NotSupported);
                }
                match menvcfg_read() {
                    Some(value) => Ok(((value & bit) != 0) as usize),
                    None => Err(Error::NotSupported),
                }
            }
            None => Err(Error::NotSupported),
        },
    }
}
