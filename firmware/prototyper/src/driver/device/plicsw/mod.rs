//! An Allwinner PLICSW with an 8-by-8 source/target interrupt matrix.

mod description;
mod device;

use runtime::{Error, FdtNode, PlatformView, Result};

pub(crate) use description::Desc;

/// The PLICSW compatible string.
pub(crate) const COMPATIBLE: &str = "allwinner,sun300i-plicsw";

/// Parses a PLICSW node's register window.
pub(crate) fn describe(platform: &PlatformView<'_>, node: FdtNode<'_, '_>) -> Result<Desc> {
    let registers = platform.device_register(node)?.ok_or(Error::InvalidArgs)?;
    Ok(Desc { registers })
}
