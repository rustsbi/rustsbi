//! `/chosen/stdout-path` lookup.

use runtime::{FdtNode, PlatformView};

/// Returns the enabled node named by `/chosen/stdout-path`, if any.
///
/// The `:options` suffix used by some device trees is ignored.
pub(crate) fn stdout_node<'a, 'tree>(
    platform: &'a PlatformView<'tree>,
) -> Option<FdtNode<'a, 'tree>> {
    let chosen = platform.find_enabled_node("/chosen")?;
    let path = chosen
        .property("stdout-path")?
        .as_str()?
        .split(':')
        .next()?;
    platform.find_enabled_node(path)
}
