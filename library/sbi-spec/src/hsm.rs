//! Chapter 9. Hart State Management Extension (EID #0x48534D "HSM").

/// Extension ID for Hart State Management extension.
#[doc(alias = "sbi_eid_hsm")]
pub const EID_HSM: usize = crate::eid_from_str("HSM") as _;
pub use fid::*;

/// Hart states.
///
/// Declared in Table 1 at §9.
pub mod hart_state {
    /// The hart is physically powered-up and executing normally.
    #[doc(alias = "sbi_started")]
    pub const STARTED: usize = 0;
    /// The hart is not executing in supervisor-mode or any lower privilege mode.
    ///
    /// It is probably powered-down by the SBI implementation if the underlying platform
    /// has a mechanism to physically power-down harts.
    #[doc(alias = "sbi_stopped")]
    pub const STOPPED: usize = 1;
    /// The hart is pending before being started
    ///
    /// Some other hart has requested to start (or power-up) the hart from the STOPPED state,
    /// and the SBI implementation is still working to get the hart in the STARTED state.
    #[doc(alias = "sbi_start_pending")]
    pub const START_PENDING: usize = 2;
    /// The hart is pending before being stopped.
    ///
    /// The hart has requested to stop (or power-down) itself from the STARTED state,
    /// and the SBI implementation is still working to get the hart in the STOPPED state.
    #[doc(alias = "sbi_stop_pending")]
    pub const STOP_PENDING: usize = 3;
    /// The hart is in a platform-specific suspend (or low-power) state.
    #[doc(alias = "sbi_suspended")]
    pub const SUSPENDED: usize = 4;
    /// The hart is pending before being suspended.
    ///
    /// The hart has requested to put itself in a platform-specific low-power state
    /// from the STARTED state, and the SBI implementation is still working to get
    /// the hart in the platform-specific SUSPENDED state.
    #[doc(alias = "sbi_suspend_pending")]
    pub const SUSPEND_PENDING: usize = 5;
    /// The hart is pending before being resumed.
    ///
    /// An interrupt or platform specific hardware event has caused the hart to resume
    /// normal execution from the SUSPENDED state, and the SBI implementation is still
    /// working to get the hart in the STARTED state.
    #[doc(alias = "sbi_resume_pending")]
    pub const RESUME_PENDING: usize = 6;
}

/// Hart suspend types.
pub mod suspend_type {
    /// Default retentive hart suspend type.
    #[doc(alias = "sbi_retentive")]
    pub const RETENTIVE: u32 = 0;
    /// Default non-retentive hart suspend type.
    #[doc(alias = "sbi_non_retentive")]
    pub const NON_RETENTIVE: u32 = 0x8000_0000;
}

/// Declared in §9.5.
mod fid {
    /// Function ID to start executing the given hart at specified address in supervisor-mode.
    ///
    /// Declared in §9.1.
    #[doc(alias = "sbi_hart_start")]
    pub const HART_START: usize = 0;
    /// Function ID to stop executing the calling hart in supervisor-mode.
    ///
    /// Declared in §9.2.
    #[doc(alias = "sbi_hart_stop")]
    pub const HART_STOP: usize = 1;
    /// Function ID to get the current status (or HSM state id) of the given hart.
    ///
    /// Declared in §9.3.
    #[doc(alias = "sbi_hart_get_status")]
    pub const HART_GET_STATUS: usize = 2;
    /// Function ID to put the calling hart into suspend or platform-specific lower power states.
    ///
    /// Declared in §9.4.
    #[doc(alias = "sbi_hart_suspend")]
    pub const HART_SUSPEND: usize = 3;
}
