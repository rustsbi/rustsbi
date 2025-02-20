//! Chapter 11. Performance Monitoring Unit Extension (EID #0x504D55 "PMU").

/// Extension ID for Performance Monitoring Unit extension.
#[doc(alias = "SBI_EXT_PMU")]
pub const EID_PMU: usize = crate::eid_from_str("PMU") as _;
pub use fid::*;

/// Declared in §11.11.
mod fid {
    /// Function ID to get the number of counters, both hardware and firmware.
    ///
    /// Declared in §11.6.
    #[doc(alias = "SBI_EXT_PMU_NUM_COUNTERS")]
    pub const NUM_COUNTERS: usize = 0;
    /// Function ID to get details about the specified counter.
    ///
    /// Declared in §11.7.
    #[doc(alias = "SBI_EXT_PMU_COUNTER_GET_INFO")]
    pub const COUNTER_GET_INFO: usize = 1;
    /// Function ID to find and configure a counter from a set of counters.
    ///
    /// Declared in §11.8.
    #[doc(alias = "SBI_EXT_PMU_COUNTER_CFG_MATCH")]
    pub const COUNTER_CONFIG_MATCHING: usize = 2;
    /// Function ID to start or enable a set of counters on the calling hart with the specified initial value.
    ///
    /// Declared in §11.9.
    #[doc(alias = "SBI_EXT_PMU_COUNTER_START")]
    pub const COUNTER_START: usize = 3;
    /// Function ID to stop or disable a set of counters on the calling hart.
    ///
    /// Declared in §11.10.
    #[doc(alias = "SBI_EXT_PMU_COUNTER_STOP")]
    pub const COUNTER_STOP: usize = 4;
    /// Function ID to provide the current value of a firmware counter.
    ///
    /// Declared in §11.11.
    #[doc(alias = "SBI_EXT_PMU_COUNTER_FW_READ")]
    pub const COUNTER_FW_READ: usize = 5;
    /// Function ID to provide the upper 32 bits from the value of the current firmware counter.
    ///
    /// Declared in §11.12.
    #[doc(alias = "SBI_EXT_PMU_COUNTER_FW_READ_HI")]
    pub const COUNTER_FW_READ_HI: usize = 6;
    /// Function ID to set and enable the PMU snapshot shared memory.
    ///
    /// Declared in §11.13.
    #[doc(alias = "SBI_EXT_PMU_SNAPSHOT_SET_SHMEM")]
    pub const SNAPSHOT_SET_SHMEM: usize = 7;
    /// Function ID to get details about any PMU event via shared memory.
    ///
    /// Declared in §11.14.
    #[doc(alias = "SBI_EXT_PMU_EVENT_GET_INFO")]
    pub const EVENT_GET_INFO: usize = 8;
}

/// PMU Event Types.
///
/// Declared in §11.
pub mod event_type {
    /// Type for all hardware general events.
    ///
    /// Declared in §11.1.
    pub const HARDWARE_GENERAL: usize = 0;
    /// Type for all hardware cache events.
    ///
    /// Declared in §11.2.
    pub const HARDWARE_CACHE: usize = 1;
    /// Type for all hardware raw events.
    ///
    /// Declared in §11.3.
    pub const HARDWARE_RAW: usize = 2;
    /// Type for all hardware raw events v2.
    ///
    /// Declared in §11.4.
    pub const HARDWARE_RAW_V2: usize = 3;
    /// Type for all firmware events.
    ///
    /// Declared in §11.5.
    pub const FIRMWARE: usize = 15;
}

/// Hardware General Event Codes.
///
/// Declared in §11.1.
pub mod hardware_event {
    /// Unused event because event_idx cannot be zero.
    pub const NO_EVENT: usize = 0;
    /// Event for each CPU cycle.
    pub const CPU_CYCLES: usize = 1;
    /// Event for each completed instruction.
    pub const INSTRUCTIONS: usize = 2;
    /// Event for cache hit.
    pub const CACHE_REFERENCES: usize = 3;
    /// Event for cache miss.
    pub const CACHE_MISSES: usize = 4;
    /// Event for a branch instruction.
    pub const BRANCH_INSTRUCTIONS: usize = 5;
    /// Event for a branch misprediction.
    pub const BRANCH_MISSES: usize = 6;
    /// Event for each BUS cycle.
    pub const BUS_CYCLES: usize = 7;
    /// Event for a stalled cycle in micro-architecture frontend.
    pub const STALLED_CYCLES_FRONTEND: usize = 8;
    /// Event for a stalled cycle in micro-architecture backend.
    pub const STALLED_CYCLES_BACKEND: usize = 9;
    /// Event for each reference CPU cycle.
    pub const REF_CPU_CYCLES: usize = 10;
}

/// Hardware Cache Event ID.
///
/// Declared in §11.2.
pub mod cache_event {
    /// Level 1 data cache event.
    pub const L1D: usize = 0;
    /// Level 1 instruction cache event.
    pub const L1I: usize = 1;
    /// Last level cache event.
    pub const LL: usize = 2;
    /// Data TLB event.
    pub const DTLB: usize = 3;
    /// Instruction TLB event.
    pub const ITLB: usize = 4;
    /// Branch predictor unit event.
    pub const BPU: usize = 5;
    /// NUMA node cache event.
    pub const NODE: usize = 6;
}

/// Hardware Cache Operation ID.
///
/// Declared in §11.2.
pub mod cache_operation {
    /// Read cache line.
    pub const READ: usize = 0;
    /// Write cache line.
    pub const WRITE: usize = 1;
    /// Prefetch cache line.
    pub const PREFETCH: usize = 2;
}

/// Hardware Cache Operation Result ID.
///
/// Declared in §11.2.
pub mod cache_result {
    /// Cache access.
    pub const ACCESS: usize = 0;
    /// Cache miss.
    pub const MISS: usize = 1;
}

/// Firmware Event Codes.
///
/// Declared in §11.5.
pub mod firmware_event {
    /// Misaligned load trap event.
    pub const MISALIGNED_LOAD: usize = 0;
    /// Misaligned store trap event.
    pub const MISALIGNED_STORE: usize = 1;
    /// Load access trap event.
    pub const ACCESS_LOAD: usize = 2;
    /// Store access trap event.
    pub const ACCESS_STORE: usize = 3;
    /// Illegal instruction trap event.
    pub const ILLEGAL_INSN: usize = 4;
    /// Set timer event.
    pub const SET_TIMER: usize = 5;
    /// Sent IPI to other HART event.
    pub const IPI_SENT: usize = 6;
    /// Received IPI from other HART event.
    pub const IPI_RECEIVED: usize = 7;
    /// Sent FENCE.I request to other HART event.
    pub const FENCE_I_SENT: usize = 8;
    /// Received FENCE.I request from other HART event.
    pub const FENCE_I_RECEIVED: usize = 9;
    /// Sent SFENCE.VMA request to other HART event.
    pub const SFENCE_VMA_SENT: usize = 10;
    /// Received SFENCE.VMA request from other HART event.
    pub const SFENCE_VMA_RECEIVED: usize = 11;
    /// Sent SFENCE.VMA with ASID request to other HART event.
    pub const SFENCE_VMA_ASID_SENT: usize = 12;
    /// Received SFENCE.VMA with ASID request from other HART event.
    pub const SFENCE_VMA_ASID_RECEIVED: usize = 13;
    /// Sent HFENCE.GVMA request to other HART event.
    pub const HFENCE_GVMA_SENT: usize = 14;
    /// Received HFENCE.GVMA request from other HART event.
    pub const HFENCE_GVMA_RECEIVED: usize = 15;
    /// Sent HFENCE.GVMA with VMID request to other HART event.
    pub const HFENCE_GVMA_VMID_SENT: usize = 16;
    /// Received HFENCE.GVMA with VMID request from other HART event.
    pub const HFENCE_GVMA_VMID_RECEIVED: usize = 17;
    /// Sent HFENCE.VVMA request to other HART event.
    pub const HFENCE_VVMA_SENT: usize = 18;
    /// Received HFENCE.VVMA request from other HART event.
    pub const HFENCE_VVMA_RECEIVED: usize = 19;
    /// Sent HFENCE.VVMA with ASID request to other HART event.
    pub const HFENCE_VVMA_ASID_SENT: usize = 20;
    /// Received HFENCE.VVMA with ASID request from other HART event.
    pub const HFENCE_VVMA_ASID_RECEIVED: usize = 21;
    /// RISC-V platform specific firmware events.
    ///
    /// The `event_data` configuration (or parameter) contains the event encoding.
    pub const PLATFORM: usize = 65535;
}

/// Size of shared memory on PMU extension set by supervisor software for current hart.
pub mod shmem_size {
    /// Size of PMU snapshot shared memory.
    ///
    /// PMU snapshot memory size must be 4096 size on all architecture XLEN configurations.
    pub const SIZE: usize = 4096;
}

/// Find and configure a matching counter.
/// Start a set of counters.
/// Stop a set of counters.
///  
/// Declared in §11.8, §11.9 and §11.10.
pub mod flags {
    use bitflags::bitflags;

    bitflags! {
        #[derive(Clone, Copy, PartialEq, Eq)]
        /// Declared in Table 37.
        pub struct CounterCfgFlags: usize {
            /// Skip the counter matching.
            const SKIP_MATCH = 1 << 0;
            /// Clear (or zero) the counter value in counter configuration.
            const CLEAR_VALUE = 1 << 1;
            /// Start the counter after configuring a matching counter.
            const AUTO_START = 1 << 2;
            /// Event counting inhibited in VU-mode.
            const SET_VUINH = 1 << 3;
            /// Event counting inhibited in VS-mode.
            const SET_VSINH = 1 << 4;
            /// Event counting inhibited in U-mode.
            const SET_UINH = 1 << 5;
            /// Event counting inhibited in S-mode.
            const SET_SINH = 1 << 6;
            /// Event counting inhibited in M-mode.
            const SET_MINH = 1 << 7;
        }
    }

    bitflags! {
        #[derive(Clone, Copy, PartialEq, Eq)]
        /// Declared in Table 39.
        pub struct CounterStartFlags: usize {
            /// Set the value of counters based on the initial_value parameter.
            const INIT_VALUE = 1 << 0;
            /// Initialize the given counters from shared memory if available.
            const INIT_SNAPSHOT = 1 << 1;
        }
    }

    bitflags! {
        #[derive(Clone, Copy, PartialEq, Eq)]
        /// Declared in Table 41.
        pub struct CounterStopFlags: usize {
            /// Reset the counter to event mapping.
            const RESET = 1 << 0;
            /// Save a snapshot of the given counter’s values in the shared memory if available.
            const TAKE_SNAPSHOT = 1 << 1;
        }
    }
}
