//! Allwinner PLICSW IPI driver.

use alloc::boxed::Box;
use runtime::{Error, PlatformView, Result, ipi::IpiDevice};

use super::plicsw;
use super::{Description, Driver, Ipi, hart_bound};
use crate::devicetree::EnabledNode;

pub(super) struct PlicSwDriver;

pub(super) static PLIC_SW: PlicSwDriver = PlicSwDriver;

impl Driver<Ipi> for PlicSwDriver {
    fn compatibles(&self) -> &'static [&'static str] {
        &["allwinner,sun300i-plicsw"]
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
        let hart_id_upper_bound = hart_bound()?;
        Ok(Some(Description::new("plicsw", registers, move |memory| {
            Ok(
                Box::new(plicsw::bind(registers, memory, hart_id_upper_bound)?)
                    as Box<dyn IpiDevice>,
            )
        })))
    }
}
