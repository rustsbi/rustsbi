//! Committed hardware values and transactional updates.

use super::feature::{ALL, COUNT, FeatureExt, PteAdPolicy};
use super::{Feature, Value, hardware};
use crate::{Error, Result, hart::Hart};

pub(super) struct State {
    /// Last committed value per feature; `None` means the hart does not support it.
    values: [Option<Value>; COUNT],
    /// ADUE value Runtime maintains when FWFT does not expose PTE A/D control.
    implicit_adue: Option<Value>,
}

impl State {
    pub(super) fn initialize(hart: &Hart) -> Result<Self> {
        let mut state = Self {
            values: ALL.map(|feature| {
                feature
                    .supported_by(hart)
                    .then(|| feature.default_value(hart.extensions()))
            }),
            implicit_adue: PteAdPolicy::for_hart(hart)
                .implicit_adue()
                .map(Value::State),
        };
        for feature in ALL {
            if let Some(value) = state.values[feature.id() as usize] {
                state.set(feature, value)?;
            }
        }
        // Svadu-only harts need ADUE enabled even though FWFT reports the
        // feature as unsupported, so this value is separate from `values`.
        state.restore_implicit_adue()?;
        Ok(state)
    }

    pub(super) fn get(&self, feature: Feature) -> Result<Value> {
        self.values[feature.id() as usize].ok_or(Error::NotSupported)?;
        hardware::read(feature)
    }

    pub(super) fn set(&mut self, feature: Feature, value: Value) -> Result<()> {
        let committed = self.values[feature.id() as usize]
            .as_mut()
            .ok_or(Error::NotSupported)?;
        if !feature.accepts(value) {
            return Err(Error::InvalidArgs);
        }
        let previous = hardware::read(feature)?;
        if previous == value {
            *committed = value;
            return Ok(());
        }
        if let Err(error) = write_and_verify(feature, value) {
            // A failed update must leave the old hardware value intact.
            // Continuing supervisor execution after failed restoration
            // would violate that contract, so this case stops firmware.
            assert!(
                hardware::write(feature, previous).is_ok()
                    && hardware::read(feature) == Ok(previous),
                "firmware features could not restore {feature:?} after a failed update"
            );
            return Err(error);
        }
        *committed = value;
        Ok(())
    }

    pub(super) fn restore(&self) -> Result<()> {
        for feature in ALL {
            let Some(value) = self.values[feature.id() as usize] else {
                continue;
            };
            restore_hardware(feature, value)?;
        }
        self.restore_implicit_adue()
    }

    fn restore_implicit_adue(&self) -> Result<()> {
        // `hardware::write` issues the required translation fences when ADUE changes.
        match self.implicit_adue {
            Some(value) => restore_hardware(Feature::PteAdHwUpdating, value),
            None => Ok(()),
        }
    }
}

fn restore_hardware(feature: Feature, value: Value) -> Result<()> {
    if hardware::read(feature) == Ok(value) {
        return Ok(());
    }
    hardware::write(feature, value)?;
    if hardware::read(feature)? != value {
        return Err(Error::NotSupported);
    }
    Ok(())
}

fn write_and_verify(feature: Feature, value: Value) -> Result<()> {
    hardware::write(feature, value)?;
    if hardware::read(feature)? == value {
        return Ok(());
    }
    match feature {
        Feature::PointerMaskingPmlen => Err(Error::InvalidArgs),
        _ => Err(Error::NotSupported),
    }
}
