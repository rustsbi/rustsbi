//! Chapter 5. Legacy Extensions (EIDs #0x00 - #0x0F).

pub use id::*;

/// §5.10
mod id {
    /// §5.1
    pub const LEGACY_SET_TIMER: usize = 0;
    /// §5.2
    pub const LEGACY_CONSOLE_PUTCHAR: usize = 1;
    /// §5.3
    pub const LEGACY_CONSOLE_GETCHAR: usize = 2;
    /// §5.4
    pub const LEGACY_CLEAR_IPI: usize = 3;
    /// §5.5
    pub const LEGACY_SEND_IPI: usize = 4;
    /// §5.6
    pub const LEGACY_REMOTE_FENCE_I: usize = 5;
    /// §5.7
    pub const LEGACY_REMOTE_SFENCE_VMA: usize = 6;
    /// §5.8
    pub const LEGACY_REMOTE_SFENCE_VMA_ASID: usize = 7;
    /// §5.9
    pub const LEGACY_SHUTDOWN: usize = 8;
}
