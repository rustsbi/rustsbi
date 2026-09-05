//! Reset drivers.

mod pmic_spacemit_p1;
mod sifive_test;

pub(crate) use pmic_spacemit_p1::I2cAddress;

use alloc::boxed::Box;

use runtime::memory::MemoryRegistry;

use crate::platform::BoardInfo;

/// Platform reset operations used by the SBI reset extension.
pub(crate) trait ResetDevice: Send {
    /// Reports a failed test run.
    fn fail(&self, code: u16) -> !;

    /// Reports a successful test run and powers off when supported.
    fn pass(&self) -> !;

    /// Restarts the platform.
    fn reset(&self) -> !;
}

pub(crate) const SIFIVE_TEST_COMPATIBLES: [&str; 1] = ["sifive,test0"];
pub(crate) const P1_PMIC_COMPATIBLES: [&str; 2] = ["spacemit,p1", "ky,spm8821"];
pub(crate) const PMIC_I2C_COMPATIBLES: [&str; 2] = ["spacemit,k1-i2c", "ky,i2c"];

/// Binds the reset device selected during platform discovery.
pub(super) fn bind(
    board: &BoardInfo,
    memory: &mut MemoryRegistry,
) -> runtime::Result<Option<Box<dyn ResetDevice>>> {
    if let Some(registers) = board.reset {
        return Ok(Some(sifive_test::bind(registers, memory)?));
    }
    if let Some((registers, pmic_address)) = board.pmic_reset {
        return Ok(Some(pmic_spacemit_p1::bind(
            registers,
            pmic_address,
            board.timebase_frequency_hz,
            memory,
        )?));
    }
    Ok(None)
}
