//! Andes PLMT timer with `MTIME` at offset 0 and per-hart `MTIMECMP` at offset 8.

mod description;
mod device;

use runtime::{Error, FdtNode, PlatformView, Result};

pub(crate) use description::Desc;

/// The PLMT compatible string.
pub(crate) const COMPATIBLE: &str = "andestech,plmt0";

/// Parses a PLMT node's register window.
pub(crate) fn describe(platform: &PlatformView<'_>, node: FdtNode<'_, '_>) -> Result<Desc> {
    let registers = platform.device_register(node)?.ok_or(Error::InvalidArgs)?;
    Ok(Desc { registers })
}
