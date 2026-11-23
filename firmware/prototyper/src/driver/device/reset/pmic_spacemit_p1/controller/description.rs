//! Resources for the I2C controller used by the P1 PMIC.

use runtime::memory::RegRange;

/// The K1 I2C controller registers and timebase needed to reach the PMIC.
#[derive(Clone, Copy)]
pub(in super::super) struct Desc {
    pub(in super::super) registers: RegRange,
    pub(in super::super) timebase_frequency_hz: Option<u32>,
}
