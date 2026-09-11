//! The `syscon-poweroff` shutdown peripheral.

use super::syscon::SysconWord;
use super::{ResetBackend, ResetError, ResetRequest, ResetType};

/// Shutdown through a masked syscon register update.
pub(crate) struct SysconPoweroff {
    word: SysconWord,
}

impl SysconPoweroff {
    pub(crate) const COMPATIBLE: &str = "syscon-poweroff";

    pub(super) fn new(word: SysconWord) -> Self {
        Self { word }
    }

    /// Issues the shutdown register update.
    pub(crate) fn poweroff(&self) -> ResetError {
        self.word.write_masked()
    }
}

impl ResetBackend for SysconPoweroff {
    type Request = ();

    fn prepare_reset(&self, req: ResetRequest) -> Option<Self::Request> {
        // The SBI layer validates the reason; this peripheral does not encode it.
        matches!(req.reset_type, ResetType::Shutdown).then_some(())
    }

    fn system_reset(&mut self, (): Self::Request) -> ResetError {
        self.poweroff()
    }
}
