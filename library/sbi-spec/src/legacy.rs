//! Chapter 5. Legacy Extensions (EIDs #0x00 - #0x0F).

pub use id::*;

/// §5.10
mod id {
    /// §5.1
    #[doc(alias = "sbi_legacy_set_timer")]
    pub const LEGACY_SET_TIMER: usize = 0;
    /// §5.2
    #[doc(alias = "sbi_legacy_console_putchar")]
    pub const LEGACY_CONSOLE_PUTCHAR: usize = 1;
    /// §5.3
    #[doc(alias = "sbi_legacy_console_getchar")]
    pub const LEGACY_CONSOLE_GETCHAR: usize = 2;
    /// §5.4
    #[doc(alias = "sbi_legacy_clear_ipi")]
    pub const LEGACY_CLEAR_IPI: usize = 3;
    /// §5.5
    #[doc(alias = "sbi_legacy_send_ipi")]
    pub const LEGACY_SEND_IPI: usize = 4;
    /// §5.6
    #[doc(alias = "sbi_legacy_remote_fence_i")]
    pub const LEGACY_REMOTE_FENCE_I: usize = 5;
    /// §5.7
    #[doc(alias = "sbi_legacy_remote_sfence_vma")]
    pub const LEGACY_REMOTE_SFENCE_VMA: usize = 6;
    /// §5.8
    #[doc(alias = "sbi_legacy_remote_sfence_vma_asid")]
    pub const LEGACY_REMOTE_SFENCE_VMA_ASID: usize = 7;
    /// §5.9
    #[doc(alias = "sbi_legacy_shutdown")]
    pub const LEGACY_SHUTDOWN: usize = 8;
}
