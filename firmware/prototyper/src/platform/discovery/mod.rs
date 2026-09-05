//! Translation from Platform Description nodes into [`BoardInfo`].

mod console;
mod devices;
mod harts;
mod imsic;

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
    board.console = console::discover(platform)?;
    devices::discover(&mut board, platform)?;
    board.spacemit_k1 = platform.spacemit_k1_registers()?;
    Ok(board)
}
