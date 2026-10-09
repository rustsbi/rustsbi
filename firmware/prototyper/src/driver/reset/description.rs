//! Devicetree discovery for reset drivers.
//!
//! [`Description`] collects reset-driver descriptions during discovery, then
//! binds only the highest-priority match into a [`ResetController`].

use runtime::Result;

use crate::devicetree::EnabledNode;

use super::registry::ResetDriver;
use super::{ResetController, pmic_spacemit_p1, sifive_test, sunxi_watchdog, syscon};

/// Reset-driver descriptions collected from one Platform Description view.
#[derive(Default)]
pub(crate) struct Description {
    sifive_test: sifive_test::SifiveTestDriver,
    p1_pmic: pmic_spacemit_p1::P1PmicDriver,
    v105: sunxi_watchdog::V105Driver,
    v104: sunxi_watchdog::V104Driver,
    syscon: syscon::SysconDriver,
}

impl Description {
    /// Lets every built-in reset driver inspect one enabled node.
    pub(crate) fn probe(
        &mut self,
        platform: &runtime::PlatformView<'_>,
        node: EnabledNode<'_, '_>,
    ) -> Result<()> {
        for driver in [
            &mut self.sifive_test as &mut dyn ResetDriver,
            &mut self.p1_pmic,
            &mut self.v105,
            &mut self.v104,
            &mut self.syscon,
        ] {
            driver.probe(platform, node)?;
        }
        Ok(())
    }

    fn selected(&self) -> Option<&dyn ResetDriver> {
        [
            &self.sifive_test as &dyn ResetDriver,
            &self.p1_pmic,
            &self.v105,
            &self.v104,
            &self.syscon,
        ]
        .into_iter()
        .find(|driver| driver.has_device())
    }

    /// Binds the first discovered driver in built-in priority order.
    pub(crate) fn bind(
        &self,
        timebase_frequency_hz: Option<u32>,
        memory: &mut runtime::memory::MemoryRegistry,
    ) -> Result<Option<ResetController>> {
        self.selected()
            .map(|driver| {
                driver
                    .bind(memory, timebase_frequency_hz)
                    .map(ResetController::new)
            })
            .transpose()
    }

    /// Logs the highest-priority discovered reset description.
    pub(crate) fn log_summary(&self) {
        if let Some(driver) = self.selected() {
            driver.log_summary();
        } else {
            warn!("{:<30}: Not Available", "Platform Reset Device");
        }
    }
}
