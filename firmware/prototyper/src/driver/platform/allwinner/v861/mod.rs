//! V861 C907 hart-release device.

mod c907;
mod description;
mod device;

use runtime::{PlatformView, Result, soc::allwinner::v861::AllwinnerV861Soc};

pub(crate) use description::Desc;

pub(crate) fn describe(platform: &PlatformView<'_>) -> Result<Option<Desc>> {
    Ok(platform.soc::<AllwinnerV861Soc>()?.map(|soc| Desc { soc }))
}
