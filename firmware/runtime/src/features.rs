//! Architectural feature discovery and supervisor environment operations.
//!
//! Firmware policy selects which supervisor facilities to expose.
//! Runtime owns their CSR layout, guarded access, and WARL-field verification.

#![forbid(unsafe_code)]

use core::fmt;
use core::marker::PhantomData;

mod stateen;

use crate::csr::{
    EnvironmentConfig, Mcounteren, Mcountinhibit, Medeleg, Menvcfg, Misa, Mseccfg, Readable,
    SecurityConfig, Value, Writable,
};
#[cfg(target_pointer_width = "32")]
use crate::csr::{EnvironmentConfigHigh, MenvcfgHigh};
use crate::hart::{HartId, HartIdError};

/// Privileged architecture version inferred from implemented standard CSRs.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum PrivilegedVersion {
    /// No supported privileged architecture version was detected.
    #[default]
    Unknown = 0,
    /// Privileged architecture version 1.10.
    Version1_10 = 1,
    /// Privileged architecture version 1.11.
    Version1_11 = 2,
    /// Privileged architecture version 1.12 or newer.
    Version1_12 = 3,
}

/// A supervisor environment feature controlled by firmware.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnvironmentFeature {
    /// Supervisor landing-pad enforcement.
    LandingPad,
    /// Supervisor shadow-stack support.
    ShadowStack,
    /// Supervisor double-trap handling.
    DoubleTrap,
    /// Hardware updates of page-table accessed and dirty bits.
    PteAdHardwareUpdating,
    /// Supervisor pointer-masking length: 0, 7, or 16 address bits.
    PointerMasking,
}

/// A failure to discover or configure an architectural feature.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FeatureError {
    /// The supplied value is not defined for the requested feature.
    InvalidValue,
    /// The feature is unavailable or its WARL field rejected the value.
    Unsupported,
    /// An architectural feature access failed.
    Access(crate::trap::Error),
    /// A register could not be restored after a probe or failed update.
    RollbackFailed {
        /// The original guarded-access failure, if one occurred.
        /// A WARL rejection or a successful probe has no guarded-access failure.
        original: Option<crate::trap::Error>,
        /// The guarded-access failure while restoring the original value.
        rollback: crate::trap::Error,
    },
}

impl fmt::Display for FeatureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidValue => formatter.write_str("invalid environment feature value"),
            Self::Unsupported => formatter.write_str("environment feature is unavailable"),
            Self::Access(error) => {
                write!(formatter, "environment feature access failed: {error:?}")
            }
            Self::RollbackFailed { original, rollback } => write!(
                formatter,
                "environment feature restore failed: {rollback:?}; original error: {original:?}"
            ),
        }
    }
}

/// Firmware policy for the current hart's supervisor environment.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SupervisorEnvironmentPolicy {
    /// Expose the page-based memory types advertised by platform discovery.
    pub page_based_memory_types: bool,
    /// Allow supervisor access to the entropy seed facility.
    pub supervisor_seed: bool,
    /// Expose supervisor AIA and IMSIC state selected by platform policy.
    pub supervisor_aia: bool,
}

/// The architectural environment of the hart executing this call chain.
///
/// The object is hart-local and cannot be sent or shared between harts.
/// Field updates preserve unrelated bits, verify WARL readback, and attempt
/// rollback on failure. Platform policy remains an explicit input.
///
/// # Panics
///
/// Operations panic if the object is used on a different hart.
pub struct SupervisorEnvironment {
    hart: HartId,
    _hart_local: PhantomData<*mut ()>,
}

impl SupervisorEnvironment {
    /// Binds environment operations to the current published hart.
    pub fn current() -> Result<Self, HartIdError> {
        Ok(Self {
            hart: HartId::current()?,
            _hart_local: PhantomData,
        })
    }

    fn assert_current(&self) {
        assert_eq!(
            HartId::current(),
            Ok(self.hart),
            "BUG: supervisor environment used on another hart"
        );
    }

    /// Estimates the current hart's privileged architecture version for diagnostics.
    ///
    /// Optional CSR presence is a heuristic and cannot establish feature support.
    /// Capability operations probe the registers they need independently.
    /// Other guarded-access errors retain their original facts in [`FeatureError`].
    pub fn probe_privileged_version(&self) -> Result<PrivilegedVersion, FeatureError> {
        self.assert_current();
        riscv::interrupt::machine::free(|| {
            if Mcounteren::read_optional()
                .map_err(FeatureError::Access)?
                .is_none()
            {
                return Ok(PrivilegedVersion::Unknown);
            }
            if Mcountinhibit::read_optional()
                .map_err(FeatureError::Access)?
                .is_none()
            {
                return Ok(PrivilegedVersion::Version1_10);
            }
            if Menvcfg::read_optional()
                .map_err(FeatureError::Access)?
                .is_none()
            {
                return Ok(PrivilegedVersion::Version1_11);
            }
            Ok(PrivilegedVersion::Version1_12)
        })
    }

    /// Returns whether this hart implements Supervisor mode.
    pub fn supports_supervisor_mode(&self) -> bool {
        self.assert_current();
        Misa::read()
            .expect("native MISA read cannot fail")
            .has_extension('S')
    }

    /// Returns whether this hart implements User mode.
    pub fn supports_user_mode(&self) -> bool {
        self.assert_current();
        Misa::read()
            .expect("native MISA read cannot fail")
            .has_extension('U')
    }

    /// Returns whether this hart implements the Hypervisor extension.
    pub fn supports_hypervisor_mode(&self) -> bool {
        self.assert_current();
        Misa::read()
            .expect("native MISA read cannot fail")
            .has_extension('H')
    }

    /// Returns whether either misaligned load/store exception is delegated.
    /// Returns [`FeatureError::Unsupported`] when Supervisor mode is absent.
    pub fn get_misaligned_delegation(&self) -> Result<usize, FeatureError> {
        self.assert_current();
        riscv::interrupt::machine::free(|| {
            if !Misa::read()
                .expect("native MISA read cannot fail")
                .has_extension('S')
            {
                return Err(FeatureError::Unsupported);
            }
            let delegation = Medeleg::read().map_err(FeatureError::Access)?;
            Ok(usize::from(
                delegation & Medeleg::MISALIGNED_EXCEPTIONS != 0,
            ))
        })
    }

    /// Enables (1) or disables (0) delegation of misaligned load/store exceptions.
    /// Other delegation bits are preserved with machine interrupts masked.
    pub fn set_misaligned_delegation(&self, value: usize) -> Result<(), FeatureError> {
        self.assert_current();
        riscv::interrupt::machine::free(|| {
            if !Misa::read()
                .expect("native MISA read cannot fail")
                .has_extension('S')
            {
                return Err(FeatureError::Unsupported);
            }
            if value > 1 {
                return Err(FeatureError::InvalidValue);
            }
            let current = Medeleg::read().map_err(FeatureError::Access)?;
            let next = if value == 1 {
                current | Medeleg::MISALIGNED_EXCEPTIONS
            } else {
                current & !Medeleg::MISALIGNED_EXCEPTIONS
            };
            Medeleg::write(next).map_err(FeatureError::Access)
        })
    }

    /// Prepares environment and state access for a fresh supervisor next stage.
    ///
    /// Run during per-hart initialization.
    /// Runtime masks machine interrupts throughout the operation.
    /// Unrelated environment fields are preserved.
    /// Absent `menvcfg`, `mseccfg`, or state-enable banks are skipped.
    /// Access failures within present banks are returned.
    pub fn configure(&self, policy: SupervisorEnvironmentPolicy) -> Result<(), FeatureError> {
        self.assert_current();
        riscv::interrupt::machine::free(|| {
            if let Some(current) = Menvcfg::read_optional().map_err(FeatureError::Access)? {
                let bits = current.bits() | EnvironmentConfig::CACHE_BLOCK_OPERATIONS;
                #[cfg(target_pointer_width = "64")]
                let bits = bits
                    | if policy.page_based_memory_types {
                        EnvironmentConfig::PAGE_BASED_MEMORY_TYPES
                    } else {
                        0
                    };
                Menvcfg::write(EnvironmentConfig::from_bits(bits)).map_err(FeatureError::Access)?;
                #[cfg(target_pointer_width = "32")]
                if policy.page_based_memory_types {
                    let high = MenvcfgHigh::read().map_err(FeatureError::Access)?;
                    MenvcfgHigh::write(EnvironmentConfigHigh::from_bits(
                        high.bits() | EnvironmentConfigHigh::PAGE_BASED_MEMORY_TYPES,
                    ))
                    .map_err(FeatureError::Access)?;
                }
            }

            if policy.supervisor_seed
                && let Some(current) = Mseccfg::read_optional().map_err(FeatureError::Access)?
            {
                Mseccfg::write(SecurityConfig::from_bits(
                    current.bits() | SecurityConfig::SUPERVISOR_SEED,
                ))
                .map_err(FeatureError::Access)?;
            }
            stateen::configure_supervisor(policy.supervisor_aia)
        })
    }

    /// Reads one firmware-controlled environment feature on the current hart.
    ///
    /// The operation probes the WARL field and attempts to restore its original
    /// value. A restoration failure returns [`FeatureError::RollbackFailed`].
    /// Machine interrupts remain masked throughout the operation.
    pub fn get(&self, feature: EnvironmentFeature) -> Result<usize, FeatureError> {
        self.assert_current();
        riscv::interrupt::machine::free(|| {
            let field = EnvironmentField::new(feature);
            #[cfg(target_pointer_width = "32")]
            if field.mask >> 32 != 0 {
                return FieldAccess::<MenvcfgHigh>::new(field.high_half())?.get();
            }
            FieldAccess::<Menvcfg>::new(field)?.get()
        })
    }

    /// Updates one firmware-controlled environment feature on the current hart.
    ///
    /// Runtime checks the value, preserves other fields, verifies WARL readback,
    /// and attempts to restore the original register on failure.
    /// A restoration failure returns [`FeatureError::RollbackFailed`].
    /// A successful change to the PTE A/D update policy synchronizes supervisor
    /// translations, and guest-physical translations when H is present.
    /// Call during initialization or a machine-mode trap.
    /// Runtime masks machine interrupts throughout the operation.
    pub fn set(&self, feature: EnvironmentFeature, value: usize) -> Result<(), FeatureError> {
        self.assert_current();
        riscv::interrupt::machine::free(|| {
            let field = EnvironmentField::new(feature);
            // Validate protocol values before accessing hardware.
            let encoded = field.encode(value)?;
            #[cfg(target_pointer_width = "32")]
            let changed = if field.mask >> 32 != 0 {
                FieldAccess::<MenvcfgHigh>::new(field.high_half())?.set(encoded)?
            } else {
                FieldAccess::<Menvcfg>::new(field)?.set(encoded)?
            };
            #[cfg(target_pointer_width = "64")]
            let changed = FieldAccess::<Menvcfg>::new(field)?.set(encoded)?;
            if changed && feature == EnvironmentFeature::PteAdHardwareUpdating {
                crate::instructions::fence::sfence_vma_all();
                if Misa::read()
                    .expect("native MISA read cannot fail")
                    .has_extension('H')
                {
                    crate::instructions::fence::hfence_gvma_all();
                }
            }
            Ok(())
        })
    }
}

/// One field in the complete 64-bit architectural environment layout.
struct EnvironmentField {
    feature: EnvironmentFeature,
    mask: u64,
    shift: usize,
}

impl EnvironmentField {
    fn new(feature: EnvironmentFeature) -> Self {
        let (mask, shift) = match feature {
            EnvironmentFeature::LandingPad => (1 << 2, 2),
            EnvironmentFeature::ShadowStack => (1 << 3, 3),
            EnvironmentFeature::DoubleTrap => (1 << 59, 59),
            EnvironmentFeature::PteAdHardwareUpdating => (1 << 61, 61),
            EnvironmentFeature::PointerMasking => (0b11 << 32, 32),
        };
        Self {
            feature,
            mask,
            shift,
        }
    }

    fn encode(&self, value: usize) -> Result<usize, FeatureError> {
        match self.feature {
            EnvironmentFeature::PointerMasking => match value {
                0 => Ok(0),
                7 => Ok(2),
                16 => Ok(3),
                _ => Err(FeatureError::InvalidValue),
            },
            _ if value <= 1 => Ok(value),
            _ => Err(FeatureError::InvalidValue),
        }
    }

    fn decode(&self, value: usize) -> Result<usize, FeatureError> {
        match self.feature {
            EnvironmentFeature::PointerMasking => match value {
                0 => Ok(0),
                2 => Ok(7),
                3 => Ok(16),
                _ => Err(FeatureError::Unsupported),
            },
            _ => Ok(value),
        }
    }

    #[cfg(target_pointer_width = "32")]
    fn high_half(mut self) -> Self {
        self.mask >>= 32;
        self.shift -= 32;
        self
    }
}

/// A field transaction retaining the original word for probes and rollback.
///
/// Unrelated fields, including unknown bits, are preserved.
struct FieldAccess<R: Readable + Writable> {
    field: EnvironmentField,
    original: R::Value,
    register: PhantomData<R>,
}

impl<R: Readable + Writable> FieldAccess<R> {
    fn new(field: EnvironmentField) -> Result<Self, FeatureError> {
        let original = R::read_optional()
            .map_err(FeatureError::Access)?
            .ok_or(FeatureError::Unsupported)?;
        Ok(Self {
            field,
            original,
            register: PhantomData,
        })
    }

    fn write_read(&self, value: usize) -> Result<usize, FeatureError> {
        let next =
            (self.original.bits() & !(self.field.mask as usize)) | (value << self.field.shift);
        R::write(R::Value::from_bits(next)).map_err(FeatureError::Access)?;
        let readback = R::read().map_err(FeatureError::Access)?;
        if readback.bits() & self.field.mask as usize != next & self.field.mask as usize {
            return Err(FeatureError::Unsupported);
        }
        Ok(next)
    }

    fn restore<T>(&self, result: Result<T, FeatureError>) -> Result<T, FeatureError> {
        match R::write(self.original) {
            Ok(()) => result,
            Err(rollback) => Err(FeatureError::RollbackFailed {
                original: match result {
                    Err(FeatureError::Access(error)) => Some(error),
                    _ => None,
                },
                rollback,
            }),
        }
    }

    fn set(self, encoded: usize) -> Result<bool, FeatureError> {
        match self.write_read(encoded) {
            Ok(next) => {
                Ok(self.original.bits() & self.field.mask as usize
                    != next & self.field.mask as usize)
            }
            Err(error) => self.restore(Err(error)),
        }
    }

    fn get(self) -> Result<usize, FeatureError> {
        // Each permitted PMLEN is independent. A different legal WARL readback
        // does not establish support for the encoding that was requested.
        let encodings: &[usize] = match self.field.feature {
            EnvironmentFeature::PointerMasking => &[2, 3],
            _ => &[1],
        };
        let result = (|| {
            for &value in encodings {
                match self.write_read(value) {
                    Ok(_) => {
                        return self.field.decode(
                            (self.original.bits() & self.field.mask as usize) >> self.field.shift,
                        );
                    }
                    Err(FeatureError::Unsupported) => {}
                    Err(error) => return Err(error),
                }
            }
            Err(FeatureError::Unsupported)
        })();
        self.restore(result)
    }
}
