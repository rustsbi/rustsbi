//! Chapter 14. CPPC Extension (EID #0x43505043 "CPPC").

/// Extension ID for CPPC Extension.
#[doc(alias = "SBI_EXT_CPPC")]
pub const EID_CPPC: usize = crate::eid_from_str("CPPC") as _;
pub use fid::*;

/// Register IDs from the [SBI CPPC register table](https://github.com/riscv-non-isa/riscv-sbi-doc/blob/v3.0/src/ext-cppc.adoc#table_cppc_registers).
pub mod register {
    /// `HighestPerformance`.
    pub const HIGHEST_PERFORMANCE: u32 = 0x00;
    /// `NominalPerformance`.
    pub const NOMINAL_PERFORMANCE: u32 = 0x01;
    /// `LowestNonlinearPerformance`.
    pub const LOW_NONLINEAR_PERFORMANCE: u32 = 0x02;
    /// `LowestPerformance`.
    pub const LOWEST_PERFORMANCE: u32 = 0x03;
    /// `GuaranteedPerformanceRegister`.
    pub const GUARANTEED_PERFORMANCE: u32 = 0x04;
    /// `DesiredPerformanceRegister`.
    pub const DESIRED_PERFORMANCE: u32 = 0x05;
    /// `MinimumPerformanceRegister`.
    pub const MIN_PERFORMANCE: u32 = 0x06;
    /// `MaximumPerformanceRegister`.
    pub const MAX_PERFORMANCE: u32 = 0x07;
    /// `PerformanceReductionToleranceRegister`.
    pub const PERFORMANCE_REDUCTION_TOLERANCE: u32 = 0x08;
    /// `TimeWindowRegister`.
    pub const TIME_WINDOW: u32 = 0x09;
    /// `CounterWraparoundTime`.
    pub const COUNTER_WRAP_TIME: u32 = 0x0a;
    /// `ReferencePerformanceCounterRegister`.
    pub const REFERENCE_COUNTER: u32 = 0x0b;
    /// `DeliveredPerformanceCounterRegister`.
    pub const DELIVERED_COUNTER: u32 = 0x0c;
    /// `PerformanceLimitedRegister`.
    pub const PERFORMANCE_LIMITED: u32 = 0x0d;
    /// `CPPCEnableRegister`.
    pub const ENABLE: u32 = 0x0e;
    /// `AutonomousSelectionEnable`.
    pub const AUTONOMOUS_SELECTION_ENABLE: u32 = 0x0f;
    /// `AutonomousActivityWindowRegister`.
    pub const AUTONOMOUS_ACTIVITY_WINDOW: u32 = 0x10;
    /// `EnergyPerformancePreferenceRegister`.
    pub const ENERGY_PERFORMANCE_PREFERENCE: u32 = 0x11;
    /// `ReferencePerformance`.
    pub const REFERENCE_PERFORMANCE: u32 = 0x12;
    /// `LowestFrequency`.
    pub const LOWEST_FREQUENCY: u32 = 0x13;
    /// `NominalFrequency`.
    pub const NOMINAL_FREQUENCY: u32 = 0x14;
    /// `TransitionLatency`.
    pub const TRANSITION_LATENCY: u32 = 0x8000_0000;
}

/// Declared in §14.
mod fid {
    /// Function ID to probe a CPPC register.
    ///
    /// Declared in §14.1.
    #[doc(alias = "SBI_EXT_CPPC_PROBE")]
    pub const PROBE: usize = 0;
    /// Function ID to read CPPC register bits.
    ///
    /// Declared in §14.2.
    #[doc(alias = "SBI_EXT_CPPC_READ")]
    pub const READ: usize = 1;
    /// Function ID to read high bits of a CPPC register.
    ///
    /// Declared in §14.3.
    #[doc(alias = "SBI_EXT_CPPC_READ_HI")]
    pub const READ_HI: usize = 2;
    /// Function ID to write to a CPPC register.
    ///
    /// Declared in §14.4.
    #[doc(alias = "SBI_EXT_CPPC_WRITE")]
    pub const WRITE: usize = 3;
}
