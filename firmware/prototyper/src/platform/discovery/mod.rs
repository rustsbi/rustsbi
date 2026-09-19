//! Translation from Platform Description nodes into [`BoardInfo`].

mod console;
mod harts;
mod imsic;
mod interrupts;

use crate::devicetree::Tree;

use super::info::BoardInfo;

/// Reads the platform facts consumed by driver and SBI initialization.
pub(super) fn discover_platform(
    platform: &runtime::PlatformView<'_>,
) -> runtime::Result<BoardInfo> {
    let root = platform.root();
    let tree = root.deserialize::<Tree>();
    let mut board = BoardInfo::empty();
    harts::discover(&mut board, &tree)?;
    board.devices.console = console::discover(platform)?;
    board.devices.reset = crate::driver::ResetDescription::discover(platform)?;
    board.soc.v821 = platform
        .soc::<runtime::soc::allwinner::v821::AllwinnerV821Soc>()?
        .map(|soc| crate::platform::allwinner::v821::Description::discover(soc, platform))
        .transpose()?;
    interrupts::discover(&mut board, platform)?;
    board.soc.spacemit_k1 = platform.spacemit_k1_registers()?;
    board.soc.v861 = platform.soc::<runtime::soc::allwinner::v861::AllwinnerV861Soc>()?;
    Ok(board)
}
