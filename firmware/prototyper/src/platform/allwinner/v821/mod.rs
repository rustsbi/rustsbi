//! Allwinner V821 platform support.
//!
//! Boot preparation, V821-only devices, and the vendor extension live in the
//! same directory. `early_init` prepares them; `final_init` registers the
//! vendor extension with its SBI subsystem.

mod boot;
pub(crate) mod devices;
pub(crate) mod vendor;

use alloc::boxed::Box;
use runtime::{
    Error, PlatformView, Result, hart::HartId, memory::MemoryRegistry,
    soc::allwinner::v821::AllwinnerV821Soc,
};

use crate::devicetree::try_for_each_enabled_node;
use crate::info::PlatformInfo;
use crate::platform::{Platform, PlatformDriver};

use boot::Description;
pub(crate) use boot::V821State;

pub(crate) struct V821Driver;

pub(crate) static V821: V821Driver = V821Driver;

impl PlatformDriver for V821Driver {
    fn matches(&self, platform: &PlatformView<'_>) -> Result<bool> {
        Ok(platform.soc::<AllwinnerV821Soc>()?.is_some())
    }

    fn create(&self) -> Box<dyn Platform> {
        Box::new(V821Platform::default())
    }
}

#[derive(Default)]
struct V821Platform {
    extension: Option<vendor::Extension>,
}

impl Platform for V821Platform {
    fn early_init(
        &mut self,
        _cold_boot: bool,
        platform: &PlatformView<'_>,
        info: &mut PlatformInfo,
        memory: &mut MemoryRegistry,
    ) -> Result<()> {
        let soc = platform
            .soc::<AllwinnerV821Soc>()?
            .ok_or(Error::InvalidArgs)?;
        let mut description = Description::new(soc);
        try_for_each_enabled_node(platform.root(), &mut |node, _| {
            description.probe(platform, node)
        })?;
        let prepared = description.prepare(HartId::count())?;

        info.memory.noncacheable_alias_offset = Some(prepared.noncacheable_alias_offset());
        devices::release_boot0_isp_sram(prepared.soc(), memory)?;
        if info.devices.interrupts.plmt.is_some() && info.devices.interrupts.plicsw.is_some() {
            devices::enable_plmt_clock(prepared.soc(), memory)?;
        }

        self.extension = Some(vendor::Extension::bind(prepared, memory)?);
        Ok(())
    }

    fn final_init(
        &mut self,
        _cold_boot: bool,
        _platform: &PlatformView<'_>,
        _info: &mut PlatformInfo,
        _memory: &mut MemoryRegistry,
    ) -> Result<()> {
        if let Some(extension) = self.extension.take() {
            crate::sbi::vendor::set_extension(Box::new(extension));
        }
        Ok(())
    }
}
