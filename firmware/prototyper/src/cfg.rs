use static_toml::static_toml;

include!(concat!(env!("OUT_DIR"), "/generated_config.rs"));

#[cfg(not(any(feature = "payload", feature = "jump")))]
pub type NextAddr = crate::cfg::config::next_addr::NextAddr;
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
