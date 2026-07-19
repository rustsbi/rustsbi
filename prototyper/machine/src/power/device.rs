//! Physical device role behind the whole-machine power capability.

use super::{PowerReason, RebootKind};

/// A bound platform device that can terminate or reset the whole machine.
///
/// Implementations own their MMIO convention. Capability code calls the
/// `can_*` query before publishing the irreversible terminal transition, so a
/// supported operation must not later return from its matching commit method.
pub(crate) trait PowerDevice: Send + Sync {
    /// Reports whether `shutdown` can commit the requested reason.
    fn can_shutdown(&self, reason: PowerReason) -> bool;

    /// Reports whether `reboot` can commit the requested kind and reason.
    fn can_reboot(&self, kind: RebootKind, reason: PowerReason) -> bool;

    /// Commits a previously accepted whole-machine shutdown.
    fn shutdown(&self, reason: PowerReason) -> !;

    /// Commits a previously accepted whole-machine reboot.
    fn reboot(&self, kind: RebootKind, reason: PowerReason) -> !;
}
