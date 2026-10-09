//! SoC-specific mechanisms and fixed resource descriptions.

use fdt::node::FdtNode;

use crate::Result;

pub mod allwinner;
pub mod spacemit;
pub mod thead;

/// A SoC capability recognized from the Platform Description root node.
///
/// Implementations retain only the authority and fixed facts associated with
/// the matched SoC. Device discovery and firmware policy remain outside
/// Runtime.
pub trait Soc: Sized {
    /// Constructs a capability when the root node identifies this SoC.
    /// Returns `Ok(None)` for a different SoC.
    ///
    /// # Errors
    ///
    /// Returns an error when the implementation cannot validate the root-node
    /// description.
    fn from_root(root: FdtNode<'_, '_>) -> Result<Option<Self>>;
}
