//! Reset drivers.

mod pmic_spacemit_p1;
mod sifive_test;

use alloc::boxed::Box;

use crate::platform::BoardInfo;
use crate::platform::mmio::Mmio;

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

pub(super) fn from_board(board: &BoardInfo) -> Option<Box<dyn ResetDevice>> {
    if let Some(base) = board.reset {
        let mmio = Mmio::within(board, base, sifive_test::SPAN)?;
        return Some(Box::new(sifive_test::SifiveTestDevice::new(mmio)));
    }
    if let Some((i2c_base, pmic_addr)) = board.pmic_reset {
        let mmio = Mmio::within(board, i2c_base, pmic_spacemit_p1::SPAN)?;
        return Some(Box::new(pmic_spacemit_p1::P1Pmic::new(mmio, pmic_addr)));
    }
    None
}
