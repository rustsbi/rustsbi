//! Committed hardware values and transactional updates.

use super::{Feature, Value, hardware};
use crate::{Error, Result, hart::Hart};

pub(super) struct State {
    values: [Option<Value>; Feature::COUNT],
}

impl State {
    pub(super) fn initialize(hart: &Hart) -> Result<Self> {
        let mut state = Self {
            values: Feature::ALL
                .map(|feature| feature.supported_by(hart).then(|| feature.default_value())),
        };
        for feature in Feature::ALL {
            if state.values[feature as usize].is_some() {
                state.set(feature, feature.default_value())?;
            }
        }
        Ok(state)
    }

    pub(super) fn get(&self, feature: Feature) -> Result<Value> {
        self.values[feature as usize].ok_or(Error::NotSupported)?;
        hardware::read(feature)
    }

    pub(super) fn set(&mut self, feature: Feature, value: Value) -> Result<()> {
        let committed = self.values[feature as usize]
            .as_mut()
            .ok_or(Error::NotSupported)?;
        if !feature.accepts(value) {
            return Err(Error::InvalidArgs);
        }
        let previous = hardware::read(feature)?;
        if previous != value {
            let result = hardware::write(feature, value).and_then(|()| {
                if hardware::read(feature)? == value {
                    Ok(())
                } else if feature == Feature::PointerMasking {
                    Err(Error::InvalidArgs)
                } else {
                    Err(Error::NotSupported)
                }
            });
            if let Err(error) = result {
                // A failed update must leave the old hardware value intact.
                // Continuing supervisor execution after failed restoration
                // would violate that contract, so this case stops firmware.
                assert!(
                    hardware::write(feature, previous).is_ok()
                        && hardware::read(feature) == Ok(previous),
                    "FWFT could not restore {feature:?} after a failed update"
                );
                return Err(error);
            }
        }
        *committed = value;
        Ok(())
    }

    pub(super) fn restore(&self) -> Result<()> {
        for feature in Feature::ALL {
            let Some(value) = self.values[feature as usize] else {
                continue;
            };
            if hardware::read(feature) != Ok(value) {
                hardware::write(feature, value)?;
                if hardware::read(feature)? != value {
                    return Err(Error::NotSupported);
                }
            }
        }
        Ok(())
    }
}
