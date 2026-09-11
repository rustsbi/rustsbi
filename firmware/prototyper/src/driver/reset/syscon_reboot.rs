//! The `syscon-reboot` reboot peripheral.

use super::syscon::SysconWord;
use super::{ResetBackend, ResetError, ResetRequest, ResetType};

/// Cold and warm reboot through a masked syscon register update.
pub(crate) struct SysconReboot {
    word: SysconWord,
}

impl SysconReboot {
    pub(crate) const COMPATIBLE: &str = "syscon-reboot";

    pub(super) fn new(word: SysconWord) -> Self {
        Self { word }
    }

    /// Issues the reboot register update.
    pub(crate) fn reboot(&self) -> ResetError {
        self.word.write_masked()
    }
}

impl ResetBackend for SysconReboot {
    type Request = ();

    fn prepare_reset(&self, req: ResetRequest) -> Option<Self::Request> {
        // Both reboot types use the same register value, regardless of reason.
        matches!(
            req.reset_type,
            ResetType::ColdReboot | ResetType::WarmReboot
        )
        .then_some(())
    }

    fn system_reset(&mut self, (): Self::Request) -> ResetError {
        self.reboot()
    }
}
