use static_toml::static_toml;

static_toml! {
    const CONFIG = include_toml!("../../target/prototyper/config.toml");
}

/// The address where the SBI link start.
pub const SBI_LINK_START_ADDRESS: usize = CONFIG.link_start_address as usize;

#[cfg(not(any(feature = "payload", feature = "jump")))]
pub type NextAddr = crate::cfg::config::next_addr::NextAddr;

/// Maximum number of supported harts; sourced from the Runtime
/// configuration so all per-hart arrays agree with the Runtime's.
pub const NUM_HART_MAX: usize = runtime::cfg::NUM_HART_MAX;
/// Heap Size of SBI firmware.
pub const HEAP_SIZE: usize = CONFIG.heap_size as usize;
/// Platform page size.
pub const PAGE_SIZE: usize = CONFIG.page_size as usize;
/// Log Level.
pub const LOG_LEVEL: &str = CONFIG.log_level;
/// Address for jump mode.
#[cfg(feature = "jump")]
pub const JUMP_ADDRESS: usize = CONFIG.jump_address as usize;
/// TLB_FLUSH_LIMIT defines the TLB refresh range limit.
/// If the TLB refresh range is greater than TLB_FLUSH_LIMIT, the entire TLB is refreshed.
pub const TLB_FLUSH_LIMIT: usize = CONFIG.tlb_flush_limit as usize;

/// The dynamic valid next addr ranges.
#[cfg(not(any(feature = "payload", feature = "jump")))]
pub const DYNAMIC_NEXT_ADDR_RANGE: &NextAddr = &CONFIG.next_addr;
