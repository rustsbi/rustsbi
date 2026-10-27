//! Andes PLMT timer driver.

use alloc::boxed::Box;
use runtime::{Error, PlatformView, Result, timer::TimerDevice};

use super::plmt;
use super::{Description, Driver, Timer, hart_id_upper_bound};
use crate::devicetree::EnabledNode;

pub(super) struct PlmtDriver;

pub(super) static PLMT: PlmtDriver = PlmtDriver;

impl Driver<Timer> for PlmtDriver {
    fn compatibles(&self) -> &'static [&'static str] {
        &["andestech,plmt0"]
    }

    fn parse<'a, 'tree>(
        &self,
        platform: &'a PlatformView<'tree>,
        device: EnabledNode<'a, 'tree>,
        _compatible: &str,
    ) -> Result<Option<Description>> {
        let registers = platform
            .device_register(device.node())?
            .ok_or(Error::InvalidArgs)?;
        let hart_id_upper_bound = hart_id_upper_bound()?;
        Ok(Some(Description::new("plmt", registers, move |memory| {
            Ok(
                Box::new(plmt::bind(registers, memory, hart_id_upper_bound)?)
                    as Box<dyn TimerDevice>,
            )
        })))
    }
}
