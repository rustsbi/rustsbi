//! Firmware feature configuration used by the SBI FWFT service.

use crate::rustsbi::spec::fwft::feature_type;
use crate::{
    Error, Result,
    csr::{csrr, csrw},
};
#[cfg(target_pointer_width = "32")]
use csr::machine::{ADUEH, DTEH, EnvironmentConfigHigh};
#[cfg(target_pointer_width = "64")]
use csr::{
    Read as CsrRead,
    machine::{ADUE, DTE, PMM, PointerMasking, PointerMaskingRead},
};
use csr::{
    Write as CsrWrite,
    machine::{self, EnvironmentConfig, FeatureState, LPE, SSE},
    values::trap::Exception,
};

/// Sets one supported firmware feature on the current hart.
pub fn set(feature_id: u32, value: usize) -> Result<()> {
    if !crate::boot::traps_ready() || usize::from(machine::Scratch::read()) != 0 {
        return Err(Error::NotReady);
    }
    crate::irq::with_local_disabled(|_| set_feature(feature_id, value))
}

/// Reads one supported firmware feature on the current hart.
/// WARL probes restore the original configuration when access succeeds.
pub fn get(feature_id: u32) -> Result<usize> {
    if !crate::boot::traps_ready() || usize::from(machine::Scratch::read()) != 0 {
        return Err(Error::NotReady);
    }
    if feature_id as usize == feature_type::MISALIGNED_EXC_DELEG {
        return get_feature(feature_id);
    }
    // The remaining supported queries temporarily write WARL configuration.
    crate::irq::with_local_disabled(|_| get_feature(feature_id))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EnvReg {
    Low,
    #[cfg(target_pointer_width = "32")]
    High,
}

fn env_read(reg: EnvReg) -> Option<usize> {
    match reg {
        // SAFETY: Runtime entry is installed; menvcfg/menvcfgh reads have no side effects.
        EnvReg::Low => csrr!(EnvironmentConfig).ok(),
        #[cfg(target_pointer_width = "32")]
        EnvReg::High => csrr!(EnvironmentConfigHigh).ok(),
    }
}

fn env_write(reg: EnvReg, value: usize) -> bool {
    match reg {
        // SAFETY: Callers change only the requested lower-mode feature bits,
        // preserving the M-mode execution and translation context.
        EnvReg::Low => csrw!(EnvironmentConfig, value).is_ok(),
        #[cfg(target_pointer_width = "32")]
        EnvReg::High => csrw!(EnvironmentConfigHigh, value).is_ok(),
    }
}

fn env_feature(feature_id: usize) -> Option<(EnvReg, usize)> {
    macro_rules! bit {
        ($reg:ident, $field:ty) => {{
            let mut reg = $reg::<CsrWrite>::default();
            reg.set::<$field>(FeatureState::Enabled);
            usize::from(reg)
        }};
    }
    match feature_id {
        feature_type::LANDING_PAD => Some((EnvReg::Low, bit!(EnvironmentConfig, LPE))),
        feature_type::SHADOW_STACK => Some((EnvReg::Low, bit!(EnvironmentConfig, SSE))),
        #[cfg(target_pointer_width = "64")]
        feature_type::DOUBLE_TRAP => Some((EnvReg::Low, bit!(EnvironmentConfig, DTE))),
        #[cfg(target_pointer_width = "32")]
        feature_type::DOUBLE_TRAP => Some((EnvReg::High, bit!(EnvironmentConfigHigh, DTEH))),
        #[cfg(target_pointer_width = "64")]
        feature_type::PTE_AD_HW_UPDATING => Some((EnvReg::Low, bit!(EnvironmentConfig, ADUE))),
        #[cfg(target_pointer_width = "32")]
        feature_type::PTE_AD_HW_UPDATING => {
            Some((EnvReg::High, bit!(EnvironmentConfigHigh, ADUEH)))
        }
        _ => None,
    }
}

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

fn probe_env_bit(reg: EnvReg, bit: usize, current: usize) -> bool {
    if !env_write(reg, current | bit) {
        return false;
    }
    let probed = env_read(reg);
    let _ = env_write(reg, current);
    probed.is_some_and(|raw| (raw & bit) != 0)
}

fn set_env_bit(feature_id: usize, reg: EnvReg, bit: usize, value: usize) -> Result<()> {
    if value > 1 {
        return Err(Error::InvalidArgs);
    }
    let current = env_read(reg).ok_or(Error::NotSupported)?;
    let target_set = value == 1;
    let prev_set = (current & bit) != 0;
    if target_set == prev_set {
        if !prev_set && !probe_env_bit(reg, bit, current) {
            return Err(Error::NotSupported);
        }
        return Ok(());
    }

    let next = if target_set {
        current | bit
    } else {
        current & !bit
    };
    if !env_write(reg, next) {
        return Err(Error::NotSupported);
    }
    let Some(read_back) = env_read(reg) else {
        let _ = env_write(reg, current);
        return Err(Error::NotSupported);
    };
    // WARL fields ignore writes when the backing extension is absent.
    if (read_back & bit) != (next & bit) {
        let _ = env_write(reg, current);
        return Err(Error::NotSupported);
    }
    if feature_id == feature_type::PTE_AD_HW_UPDATING {
        crate::instruction::sfence_vma_all();
        if crate::hart::has_hypervisor() {
            let _ = crate::fence::execute_local(crate::fence::LocalFence::GuestPhysical {
                range: crate::fence::FlushRange::All,
                vmid: None,
            });
        }
    }
    Ok(())
}

fn get_env_bit(reg: EnvReg, bit: usize) -> Result<usize> {
    let current = env_read(reg).ok_or(Error::NotSupported)?;
    if (current & bit) != 0 {
        return Ok(1);
    }
    if probe_env_bit(reg, bit, current) {
        Ok(0)
    } else {
        Err(Error::NotSupported)
    }
}

#[cfg(target_pointer_width = "64")]
fn pmm_supported(current: usize) -> bool {
    for candidate in [PointerMasking::Bits7, PointerMasking::Bits16] {
        let mut reg =
            EnvironmentConfig::<CsrWrite>::from(EnvironmentConfig::<CsrRead>::from_bits(current));
        reg.set::<PMM>(candidate);
        if !env_write(EnvReg::Low, usize::from(reg)) {
            continue;
        }
        let read_back = env_read(EnvReg::Low)
            .map(EnvironmentConfig::<CsrRead>::from_bits)
            .map(|val| val.get::<PMM>());
        let _ = env_write(EnvReg::Low, current);
        if matches!(
            read_back,
            Some(PointerMaskingRead::Bits7 | PointerMaskingRead::Bits16)
        ) {
            return true;
        }
    }
    false
}

#[cfg(target_pointer_width = "64")]
fn pmm_pmlen(read: PointerMaskingRead) -> Option<usize> {
    match read {
        PointerMaskingRead::Disabled => Some(PointerMasking::Disabled.pmlen()),
        PointerMaskingRead::Bits7 => Some(PointerMasking::Bits7.pmlen()),
        PointerMaskingRead::Bits16 => Some(PointerMasking::Bits16.pmlen()),
        PointerMaskingRead::Unknown(_) => None,
    }
}

#[cfg(target_pointer_width = "64")]
fn set_pmm(value: usize) -> Result<()> {
    let Some(target) = PointerMasking::from_pmlen(value) else {
        return Err(Error::InvalidArgs);
    };
    let current = env_read(EnvReg::Low).ok_or(Error::NotSupported)?;
    let current_pmm = EnvironmentConfig::<CsrRead>::from_bits(current).get::<PMM>();
    if pmm_pmlen(current_pmm) == Some(value) {
        if target == PointerMasking::Disabled && !pmm_supported(current) {
            return Err(Error::NotSupported);
        }
        return Ok(());
    }

    let mut reg =
        EnvironmentConfig::<CsrWrite>::from(EnvironmentConfig::<CsrRead>::from_bits(current));
    reg.set::<PMM>(target);
    if !env_write(EnvReg::Low, usize::from(reg)) {
        return Err(Error::NotSupported);
    }
    let Some(read_back) = env_read(EnvReg::Low) else {
        let _ = env_write(EnvReg::Low, current);
        return Err(Error::NotSupported);
    };
    let observed = EnvironmentConfig::<CsrRead>::from_bits(read_back).get::<PMM>();
    if pmm_pmlen(observed) != Some(value) {
        let _ = env_write(EnvReg::Low, current);
        return Err(Error::NotSupported);
    }
    Ok(())
}

#[cfg(target_pointer_width = "64")]
fn get_pmm() -> Result<usize> {
    let current = env_read(EnvReg::Low).ok_or(Error::NotSupported)?;
    match EnvironmentConfig::<CsrRead>::from_bits(current).get::<PMM>() {
        PointerMaskingRead::Disabled => {
            if pmm_supported(current) {
                Ok(0)
            } else {
                Err(Error::NotSupported)
            }
        }
        other => pmm_pmlen(other).ok_or(Error::NotSupported),
    }
}

#[cfg(target_pointer_width = "32")]
fn set_pmm(_value: usize) -> Result<()> {
    Err(Error::NotSupported)
}

#[cfg(target_pointer_width = "32")]
fn get_pmm() -> Result<usize> {
    Err(Error::NotSupported)
}

fn is_denied_feature(feature_id: u32) -> bool {
    // Higher SBI v3.0 feature IDs are reserved or platform-specific.
    feature_id > feature_type::POINTER_MASKING_PMLEN as u32
}

fn set_feature(feature_id: u32, value: usize) -> Result<()> {
    if is_denied_feature(feature_id) {
        return Err(Error::AccessDenied);
    }
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
        _ => match env_feature(feature_id as usize) {
            Some((reg, bit)) => set_env_bit(feature_id as usize, reg, bit, value),
            None => Err(Error::NotSupported),
        },
    }
}

fn get_feature(feature_id: u32) -> Result<usize> {
    if is_denied_feature(feature_id) {
        return Err(Error::AccessDenied);
    }
    match feature_id as usize {
        feature_type::MISALIGNED_EXC_DELEG => {
            if !has_s_mode() {
                return Err(Error::NotSupported);
            }
            Ok(misaligned_delegated() as usize)
        }
        feature_type::POINTER_MASKING_PMLEN => get_pmm(),
        _ => match env_feature(feature_id as usize) {
            Some((reg, bit)) => get_env_bit(reg, bit),
            None => Err(Error::NotSupported),
        },
    }
}
