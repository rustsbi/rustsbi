//! CSR access and synchronization for the current hart's feature transaction.

use super::{Feature, PointerMasking, Value};
use crate::fault_recovery::{csrr, csrw};
use crate::{Error, Result};
use csr::{Read, Write, machine};
use machine::{EnvironmentConfig as Low, FeatureState};

#[cfg(target_pointer_width = "64")]
use machine::{
    ADUE, DTE, EnvironmentConfig as Upper,
};
#[cfg(target_pointer_width = "32")]
use machine::{
    ADUEH as ADUE, DTEH as DTE, EnvironmentConfigHigh as Upper,
};

pub(super) fn read(feature: Feature) -> Result<Value> {
    let enabled = match feature {
        Feature::MisalignedDelegation => {
            // SAFETY: the Runtime entry contract and IRQ scope hold.
            let value =
                csrr!(machine::ExceptionDelegation).map(csr::values::trap::ExceptionSet::from)?;
            use csr::values::trap::Exception;
            let load = value.contains(Exception::LoadMisaligned);
            let store = value.contains(Exception::StoreMisaligned);
            if load != store {
                return Err(Error::InvalidState);
            }
            load
        }
        // SAFETY: the FWFT transaction owns these fixed, side-effect-free reads.
        Feature::LandingPad => {
            csrr!(Low)
                .map(machine::EnvironmentConfig::<Read>::from_bits)?
                .read_field::<machine::LPE>()
                == FeatureState::Enabled
        }
        Feature::ShadowStack => {
            csrr!(Low)
                .map(machine::EnvironmentConfig::<Read>::from_bits)?
                .read_field::<machine::SSE>()
                == FeatureState::Enabled
        }
        Feature::DoubleTrap => {
            csrr!(Upper).map(Upper::<Read>::from_bits)?.read_field::<DTE>() == FeatureState::Enabled
        }
        Feature::PteAdUpdating => {
            csrr!(Upper).map(Upper::<Read>::from_bits)?.read_field::<ADUE>() == FeatureState::Enabled
        }
        Feature::PointerMasking => return read_pointer_masking().map(Value::PointerMasking),
    };
    Ok(Value::Enabled(enabled))
}

pub(super) fn write(feature: Feature, value: Value) -> Result<()> {
    match (feature, value) {
        (Feature::MisalignedDelegation, Value::Enabled(enabled)) => {
            use csr::values::trap::Exception;
            // SAFETY: the Runtime entry contract and IRQ scope hold.
            let mut value =
                csrr!(machine::ExceptionDelegation).map(csr::values::trap::ExceptionSet::from)?;
            value.set(Exception::LoadMisaligned, enabled);
            value.set(Exception::StoreMisaligned, enabled);
            // SAFETY: only the requested delegation fields change. No lower
            // context executes during this IRQ-scoped transaction.
            csrw!(machine::ExceptionDelegation, usize::from(value))
        }
        (Feature::LandingPad | Feature::ShadowStack, Value::Enabled(enabled)) => {
            // SAFETY: the IRQ-scoped transaction owns this configuration update.
            let mut value = machine::EnvironmentConfig::<Write>::from(
                csrr!(Low).map(machine::EnvironmentConfig::<Read>::from_bits)?,
            );
            if feature == Feature::LandingPad {
                value.write_field::<machine::LPE>(enabled.into());
            } else {
                value.write_field::<machine::SSE>(enabled.into());
            }
            // SAFETY: unrelated fields and the active machine context are preserved.
            csrw!(Low, usize::from(value))
        }
        (Feature::DoubleTrap | Feature::PteAdUpdating, Value::Enabled(enabled)) => {
            // SAFETY: the IRQ-scoped transaction owns this configuration update.
            let mut value = Upper::<Write>::from(csrr!(Upper).map(Upper::<Read>::from_bits)?);
            if feature == Feature::DoubleTrap {
                value.write_field::<DTE>(enabled.into());
            } else {
                value.write_field::<ADUE>(enabled.into());
            }
            // SAFETY: only the selected lower-mode feature changes, with MPRV clear.
            csrw!(Upper, usize::from(value))?;
            if feature == Feature::PteAdUpdating {
                synchronize_translation();
            }
            Ok(())
        }
        (Feature::PointerMasking, Value::PointerMasking(mode)) => write_pointer_masking(mode),
        _ => Err(Error::InvalidArgs),
    }
}
fn read_pointer_masking() -> Result<PointerMasking> {
    #[cfg(target_pointer_width = "64")]
    {
        // SAFETY: the Runtime entry contract holds; this read has no effects.
        let value = csrr!(Low).map(machine::EnvironmentConfig::<Read>::from_bits)?;
        match value.read_field::<machine::PMM>() {
            machine::PointerMaskingRead::Disabled => Ok(PointerMasking::Disabled),
            machine::PointerMaskingRead::Bits7 => Ok(PointerMasking::Bits7),
            machine::PointerMaskingRead::Bits16 => Ok(PointerMasking::Bits16),
            machine::PointerMaskingRead::Unknown(_) => Err(Error::InvalidState),
        }
    }
    #[cfg(target_pointer_width = "32")]
    Err(Error::NotSupported)
}

fn write_pointer_masking(mode: PointerMasking) -> Result<()> {
    #[cfg(target_pointer_width = "64")]
    {
        // SAFETY: IRQ exclusion covers this field-preserving update.
        let mut value = machine::EnvironmentConfig::<Write>::from(
            csrr!(Low).map(machine::EnvironmentConfig::<Read>::from_bits)?,
        );
        value.write_field::<machine::PMM>(mode);
        // SAFETY: only supervisor pointer masking changes, with no active
        // MPRV memory access or lower-mode instruction in this transaction.
        csrw!(Low, usize::from(value))
    }
    #[cfg(target_pointer_width = "32")]
    {
        let _ = mode;
        Err(Error::NotSupported)
    }
}

fn synchronize_translation() {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    // SAFETY: this M-mode configuration transaction owns translation changes.
    unsafe {
        core::arch::asm!("sfence.vma x0, x0", options(nostack));
        if machine::Isa::read().has_extension('H') {
            core::arch::asm!(
                ".option push",
                ".option arch, +h",
                "hfence.gvma x0, x0",
                ".option pop",
                options(nostack)
            );
        }
    }
}
