//! Chapter 11. Performance Monitoring Unit Extension (EID #0x504D55 "PMU").

/// Extension ID for Performance Monitoring Unit extension.
#[doc(alias = "sbi_eid_pmu")]
pub const EID_PMU: usize = crate::eid_from_str("PMU") as _;
pub use fid::*;

/// Declared in §11.11.
mod fid {
    /// Function ID to get the number of counters, both hardware and firmware.
    ///
    /// Declared in §11.6.
    #[doc(alias = "sbi_num_counters")]
    pub const NUM_COUNTERS: usize = 0;
    /// Function ID to get details about the specified counter.
    ///
    /// Declared in §11.7.
    #[doc(alias = "sbi_counter_get_info")]
    pub const COUNTER_GET_INFO: usize = 1;
    /// Function ID to find and configure a counter from a set of counters.
    ///
    /// Declared in §11.8.
    #[doc(alias = "sbi_counter_config_matching")]
    pub const COUNTER_CONFIG_MATCHING: usize = 2;
    /// Function ID to start or enable a set of counters on the calling hart with the specified initial value.
    ///
    /// Declared in §11.9.
    #[doc(alias = "sbi_counter_start")]
    pub const COUNTER_START: usize = 3;
    /// Function ID to stop or disable a set of counters on the calling hart.
    ///
    /// Declared in §11.10.
    #[doc(alias = "sbi_counter_stop")]
    pub const COUNTER_STOP: usize = 4;
    /// Function ID to provide the current value of a firmware counter.
    ///
    /// Declared in §11.11.
    #[doc(alias = "sbi_counter_fw_read")]
    pub const COUNTER_FW_READ: usize = 5;
    /// Function ID to provide the upper 32 bits from the value of the current firmware counter.
    ///
    /// Declared in §11.12.
    #[doc(alias = "sbi_counter_fw_read_hi")]
    pub const COUNTER_FW_READ_HI: usize = 6;
    /// Function ID to set and enable the PMU snapshot shared memory.
    ///
    /// Declared in §11.13.
    #[doc(alias = "sbi_snapshot_set_shmem")]
    pub const SNAPSHOT_SET_SHMEM: usize = 7;
    /// Function ID to get details about any PMU event via shared memory.
    ///
    /// Declared in §11.14.
    #[doc(alias = "sbi_event_get_info")]
    pub const EVENT_GET_INFO: usize = 8;
}

/// PMU Event Types.
///
/// Declared in §11.
pub mod event_type {
    /// Type for all hardware general events.
    ///
    /// Declared in §11.1.
    #[doc(alias = "sbi_hardware_general")]
    pub const HARDWARE_GENERAL: usize = 0;
    /// Type for all hardware cache events.
    ///
    /// Declared in §11.2.
    #[doc(alias = "sbi_hardware_cache")]
    pub const HARDWARE_CACHE: usize = 1;
    /// Type for all hardware raw events.
    ///
    /// Declared in §11.3.
    #[doc(alias = "sbi_hardware_raw")]
    pub const HARDWARE_RAW: usize = 2;
    /// Type for all hardware raw events v2.
    ///
    /// Declared in §11.4.
    #[doc(alias = "sbi_hardware_raw_v2")]
    pub const HARDWARE_RAW_V2: usize = 3;
    /// Type for all firmware events.
    ///
    /// Declared in §11.5.
    #[doc(alias = "sbi_firmware")]
    pub const FIRMWARE: usize = 15;
}

/// Hardware General Event Codes.
///
/// Declared in §11.1.
pub mod hardware_event {
    /// Unused event because event_idx cannot be zero.
    #[doc(alias = "sbi_no_event")]
    pub const NO_EVENT: usize = 0;
    /// Event for each CPU cycle.
    #[doc(alias = "sbi_cpu_cycles")]
    pub const CPU_CYCLES: usize = 1;
    /// Event for each completed instruction.
    #[doc(alias = "sbi_instructions")]
    pub const INSTRUCTIONS: usize = 2;
    /// Event for cache hit.
    #[doc(alias = "sbi_cache_references")]
    pub const CACHE_REFERENCES: usize = 3;
    /// Event for cache miss.
    #[doc(alias = "sbi_cache_misses")]
    pub const CACHE_MISSES: usize = 4;
    /// Event for a branch instruction.
    #[doc(alias = "sbi_branch_instructions")]
    pub const BRANCH_INSTRUCTIONS: usize = 5;
    /// Event for a branch misprediction.
    #[doc(alias = "sbi_branch_misses")]
    pub const BRANCH_MISSES: usize = 6;
    /// Event for each BUS cycle.
    #[doc(alias = "sbi_bus_cycles")]
    pub const BUS_CYCLES: usize = 7;
    /// Event for a stalled cycle in micro-architecture frontend.
    #[doc(alias = "sbi_stalled_cycles_frontend")]
    pub const STALLED_CYCLES_FRONTEND: usize = 8;
    /// Event for a stalled cycle in micro-architecture backend.
    #[doc(alias = "sbi_stalled_cycles_backend")]
    pub const STALLED_CYCLES_BACKEND: usize = 9;
    /// Event for each reference CPU cycle.
    #[doc(alias = "sbi_ref_cpu_cycles")]
    pub const REF_CPU_CYCLES: usize = 10;
}

/// Hardware Cache Event ID.
///
/// Declared in §11.2.
pub mod cache_event {
    /// Level 1 data cache event.
    #[doc(alias = "sbi_l1d")]
    pub const L1D: usize = 0;
    /// Level 1 instruction cache event.
    #[doc(alias = "sbi_l1i")]
    pub const L1I: usize = 1;
    /// Last level cache event.
    #[doc(alias = "sbi_ll")]
    pub const LL: usize = 2;
    /// Data TLB event.
    #[doc(alias = "sbi_dtlb")]
    pub const DTLB: usize = 3;
    /// Instruction TLB event.
    #[doc(alias = "sbi_itlb")]
    pub const ITLB: usize = 4;
    /// Branch predictor unit event.
    #[doc(alias = "sbi_bpu")]
    pub const BPU: usize = 5;
    /// NUMA node cache event.
    #[doc(alias = "sbi_node")]
    pub const NODE: usize = 6;
}

/// Hardware Cache Operation ID.
///
/// Declared in §11.2.
pub mod cache_operation {
    /// Read cache line.
    #[doc(alias = "sbi_read")]
    pub const READ: usize = 0;
    /// Write cache line.
    #[doc(alias = "sbi_write")]
    pub const WRITE: usize = 1;
    /// Prefetch cache line.
    #[doc(alias = "sbi_prefetch")]
    pub const PREFETCH: usize = 2;
}

/// Hardware Cache Operation Result ID.
///
/// Declared in §11.2.
pub mod cache_result {
    /// Cache access.
    #[doc(alias = "sbi_access")]
    pub const ACCESS: usize = 0;
    /// Cache miss.
    #[doc(alias = "sbi_miss")]
    pub const MISS: usize = 1;
}

/// Firmware Event Codes.
///
/// Declared in §11.5.
pub mod firmware_event {
    /// Misaligned load trap event.
    #[doc(alias = "sbi_misaligned_load")]
    pub const MISALIGNED_LOAD: usize = 0;
    /// Misaligned store trap event.
    #[doc(alias = "sbi_misaligned_store")]
    pub const MISALIGNED_STORE: usize = 1;
    /// Load access trap event.
    #[doc(alias = "sbi_access_load")]
    pub const ACCESS_LOAD: usize = 2;
    /// Store access trap event.
    #[doc(alias = "sbi_access_store")]
    pub const ACCESS_STORE: usize = 3;
    /// Illegal instruction trap event.
    #[doc(alias = "sbi_illegal_insn")]
    pub const ILLEGAL_INSN: usize = 4;
    /// Set timer event.
    #[doc(alias = "sbi_set_timer")]
    pub const SET_TIMER: usize = 5;
    /// Sent IPI to other HART event.
    #[doc(alias = "sbi_ipi_sent")]
    pub const IPI_SENT: usize = 6;
    /// Received IPI from other HART event.
    #[doc(alias = "sbi_ipi_received")]
    pub const IPI_RECEIVED: usize = 7;
    /// Sent FENCE.I request to other HART event.
    #[doc(alias = "sbi_fence_i_sent")]
    pub const FENCE_I_SENT: usize = 8;
    /// Received FENCE.I request from other HART event.
    #[doc(alias = "sbi_fence_i_received")]
    pub const FENCE_I_RECEIVED: usize = 9;
    /// Sent SFENCE.VMA request to other HART event.
    #[doc(alias = "sbi_sfence_vma_sent")]
    pub const SFENCE_VMA_SENT: usize = 10;
    /// Received SFENCE.VMA request from other HART event.
    #[doc(alias = "sbi_sfence_vma_received")]
    pub const SFENCE_VMA_RECEIVED: usize = 11;
    /// Sent SFENCE.VMA with ASID request to other HART event.
    #[doc(alias = "sbi_sfence_vma_asid_sent")]
    pub const SFENCE_VMA_ASID_SENT: usize = 12;
    /// Received SFENCE.VMA with ASID request from other HART event.
    #[doc(alias = "sbi_sfence_vma_asid_received")]
    pub const SFENCE_VMA_ASID_RECEIVED: usize = 13;
    /// Sent HFENCE.GVMA request to other HART event.
    #[doc(alias = "sbi_hfence_gvma_sent")]
    pub const HFENCE_GVMA_SENT: usize = 14;
    /// Received HFENCE.GVMA request from other HART event.
    #[doc(alias = "sbi_hfence_gvma_received")]
    pub const HFENCE_GVMA_RECEIVED: usize = 15;
    /// Sent HFENCE.GVMA with VMID request to other HART event.
    #[doc(alias = "sbi_hfence_gvma_vmid_sent")]
    pub const HFENCE_GVMA_VMID_SENT: usize = 16;
    /// Received HFENCE.GVMA with VMID request from other HART event.
    #[doc(alias = "sbi_hfence_gvma_vmid_received")]
    pub const HFENCE_GVMA_VMID_RECEIVED: usize = 17;
    /// Sent HFENCE.VVMA request to other HART event.
    #[doc(alias = "sbi_hfence_vvma_sent")]
    pub const HFENCE_VVMA_SENT: usize = 18;
    /// Received HFENCE.VVMA request from other HART event.
    #[doc(alias = "sbi_hfence_vvma_received")]
    pub const HFENCE_VVMA_RECEIVED: usize = 19;
    /// Sent HFENCE.VVMA with ASID request to other HART event.
    #[doc(alias = "sbi_hfence_vvma_asid_sent")]
    pub const HFENCE_VVMA_ASID_SENT: usize = 20;
    /// Received HFENCE.VVMA with ASID request from other HART event.
    #[doc(alias = "sbi_hfence_vvma_asid_received")]
    pub const HFENCE_VVMA_ASID_RECEIVED: usize = 21;
    /// RISC-V platform specific firmware events.
    ///
    /// The `event_data` configuration (or parameter) contains the event encoding.
    #[doc(alias = "sbi_platform")]
    pub const PLATFORM: usize = 65535;
}

/// Size of shared memory on PMU extension set by supervisor software for current hart.
pub mod shmem_size {
    /// Size of PMU snapshot shared memory.
    ///
    /// PMU snapshot memory size must be 4096 size on all architecture XLEN configurations.
    #[doc(alias = "sbi_size")]
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
            #[doc(alias = "sbi_skip_match")]
            const SKIP_MATCH = 1 << 0;
            /// Clear (or zero) the counter value in counter configuration.
            #[doc(alias = "sbi_clear_value")]
            const CLEAR_VALUE = 1 << 1;
            /// Start the counter after configuring a matching counter.
            #[doc(alias = "sbi_auto_start")]
            const AUTO_START = 1 << 2;
            /// Event counting inhibited in VU-mode.
            #[doc(alias = "sbi_set_vuinh")]
            const SET_VUINH = 1 << 3;
            /// Event counting inhibited in VS-mode.
            #[doc(alias = "sbi_set_vsinh")]
            const SET_VSINH = 1 << 4;
            /// Event counting inhibited in U-mode.
            #[doc(alias = "sbi_set_uinh")]
            const SET_UINH = 1 << 5;
            /// Event counting inhibited in S-mode.
            #[doc(alias = "sbi_set_sinh")]
            const SET_SINH = 1 << 6;
            /// Event counting inhibited in M-mode.
            #[doc(alias = "sbi_set_minh")]
            const SET_MINH = 1 << 7;
        }
    }

    bitflags! {
        #[derive(Clone, Copy, PartialEq, Eq)]
        /// Declared in Table 39.
        pub struct CounterStartFlags: usize {
            /// Set the value of counters based on the initial_value parameter.
            #[doc(alias = "sbi_init_value")]
            const INIT_VALUE = 1 << 0;
            /// Initialize the given counters from shared memory if available.
            #[doc(alias = "sbi_init_snapshot")]
            const INIT_SNAPSHOT = 1 << 1;
        }
    }

    bitflags! {
        #[derive(Clone, Copy, PartialEq, Eq)]
        /// Declared in Table 41.
        pub struct CounterStopFlags: usize {
            /// Reset the counter to event mapping.
            #[doc(alias = "sbi_reset")]
            const RESET = 1 << 0;
            /// Save a snapshot of the given counter’s values in the shared memory if available.
            #[doc(alias = "sbi_take_snapshot")]
            const TAKE_SNAPSHOT = 1 << 1;
        }
    }
}
