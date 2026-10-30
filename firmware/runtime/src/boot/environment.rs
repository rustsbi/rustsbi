//! Initial supervisor environment owned by the current hart.

use crate::{
    Error, Result,
    fault_recovery::{csrr, csrw},
    hart::{Hart, IsaExtensions},
};
use csr::{
    Read, Write,
    machine::{self, EnvironmentConfig as EnvConfig},
};
#[cfg(target_pointer_width = "64")]
use machine::{EnvironmentConfig as Upper, PBMTE};
#[cfg(target_pointer_width = "32")]
use machine::{EnvironmentConfigHigh as Upper, PBMTEH as PBMTE};

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
        let mut value = Upper::<Write>::from(csrr!(Upper).map(Upper::<Read>::from_bits)?);
        value.write_field::<PBMTE>(machine::FeatureState::Enabled);
        csrw!(Upper, usize::from(value))?;
        if csrr!(Upper).map(Upper::<Read>::from_bits)?.read_field::<PBMTE>()
            != machine::FeatureState::Enabled
        {
            return Err(Error::NotSupported);
        }
        Ok(())
    }

    /// Grants the cache-block operations reported for this hart.
    pub(crate) fn configure_cache_blocks(&self) -> Result<()> {
        let extensions = self.extensions();
        let current = match csrr!(EnvConfig).map(EnvConfig::<Read>::from_bits) {
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
        let mut value = EnvConfig::<Write>::from(current);
        value.write_field::<machine::CBIE>(if management {
            machine::CacheBlockInvalidateMode::Invalidate
        } else {
            machine::CacheBlockInvalidateMode::Disabled
        });
        value.write_field::<machine::CBCFE>(management.into());
        value.write_field::<machine::CBZE>(zero.into());
        csrw!(EnvConfig, usize::from(value))?;
        let actual = csrr!(EnvConfig).map(EnvConfig::<Read>::from_bits)?;
        if actual.read_field::<machine::CBIE>() != value.read_field::<machine::CBIE>()
            || actual.read_field::<machine::CBCFE>() != value.read_field::<machine::CBCFE>()
            || actual.read_field::<machine::CBZE>() != value.read_field::<machine::CBZE>()
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
        unsafe { machine::SecurityConfig::set_bit::<machine::SSEED>() };
        if csrr!(machine::SecurityConfig)
            .map(machine::SecurityConfig::<Read>::from_bits)?
            .read_field::<machine::SSEED>()
            != machine::FeatureState::Enabled
        {
            return Err(Error::NotSupported);
        }
        Ok(())
    }
}
