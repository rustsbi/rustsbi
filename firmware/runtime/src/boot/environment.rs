//! Initial supervisor environment owned by the current hart.

use crate::{
    Error, Result,
    hart::{Hart, IsaExtensions},
    recovery::{csrr, csrw},
};
use csr::machine::{
    self, EnvironmentConfig as EnvConfig, EnvironmentConfigReadValue as EnvConfigRead,
    EnvironmentConfigWriteValue as EnvConfigWrite,
};
#[cfg(target_pointer_width = "64")]
use machine::{
    EnvironmentConfig as Upper, EnvironmentConfigReadValue as UpperReadValue,
    EnvironmentConfigWriteValue as UpperValue, PBMTE,
};
#[cfg(target_pointer_width = "32")]
use machine::{
    EnvironmentConfigHigh as Upper, EnvironmentConfigHighReadValue as UpperReadValue,
    EnvironmentConfigHighWriteValue as UpperValue, PBMTEH as PBMTE,
};

impl Hart {
    /// Enables standard page attributes before any lower-mode address space exists.
    pub(crate) fn configure_page_tables(&self) -> Result<()> {
        if cfg!(target_pointer_width = "32") || self.extensions().contains(IsaExtensions::SVPBMT) {
            // SAFETY: Initial Runtime setup owns translation, excludes IRQs,
            // and has no lower-mode context. Standard page tables require MAEE off.
            unsafe { super::thead::disable_maee() };
        }
        if !self.extensions().contains(IsaExtensions::SVPBMT) {
            return Ok(());
        }
        // On RV32, probe the optional high CSR only for a reported high-half feature.
        let mut value = UpperValue::from(csrr!(Upper).map(UpperReadValue::from_bits)?);
        value.write::<PBMTE>(machine::FeatureState::Enabled);
        csrw!(Upper, usize::from(value))?;
        if csrr!(Upper).map(UpperReadValue::from_bits)?.read::<PBMTE>()
            != machine::FeatureState::Enabled
        {
            return Err(Error::NotSupported);
        }
        Ok(())
    }

    /// Grants the cache-block operations reported for this hart.
    pub(crate) fn configure_cache_blocks(&self) -> Result<()> {
        let extensions = self.extensions();
        let current = match csrr!(EnvConfig).map(EnvConfigRead::from_bits) {
            Ok(value) => value,
            Err(Error::IllegalInstruction)
                if !extensions.intersects(
                    IsaExtensions::ZICBOM | IsaExtensions::ZICBOZ | IsaExtensions::SVPBMT,
                ) =>
            {
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        let management = extensions.contains(IsaExtensions::ZICBOM);
        let zero = extensions.contains(IsaExtensions::ZICBOZ);
        let mut value = EnvConfigWrite::from(current);
        value.write::<machine::CBIE>(if management {
            machine::CacheBlockInvalidateMode::Invalidate
        } else {
            machine::CacheBlockInvalidateMode::Disabled
        });
        value.write::<machine::CBCFE>(management.into());
        value.write::<machine::CBZE>(zero.into());
        csrw!(EnvConfig, usize::from(value))?;
        let actual = csrr!(EnvConfig).map(EnvConfigRead::from_bits)?;
        if actual.read::<machine::CBIE>() != value.read::<machine::CBIE>()
            || actual.read::<machine::CBCFE>() != value.read::<machine::CBCFE>()
            || actual.read::<machine::CBZE>() != value.read::<machine::CBZE>()
        {
            return Err(Error::NotSupported);
        }
        Ok(())
    }

    /// Grants supervisor entropy-source access when Zkr is reported.
    pub(crate) fn configure_seed_access(&self) -> Result<()> {
        if !self.extensions().contains(IsaExtensions::ZKR) {
            return Ok(());
        }
        csrr!(machine::SecurityConfig)?;
        // SAFETY: Initial setup owns the seed grant and the CSR probe succeeded.
        unsafe { machine::SecurityConfig::enable_supervisor_seed() };
        if csrr!(machine::SecurityConfig)
            .map(machine::SecurityConfigReadValue::from_bits)?
            .read::<machine::SSEED>()
            != machine::FeatureState::Enabled
        {
            return Err(Error::NotSupported);
        }
        Ok(())
    }
}
