use static_toml::static_toml;

include!(concat!(env!("OUT_DIR"), "/generated_config.rs"));

#[cfg(not(any(feature = "payload", feature = "jump")))]
pub type NextAddr = crate::cfg::config::next_addr::NextAddr;

/// Configured firmware log level.
pub const LOG_LEVEL: &str = CONFIG.log_level;
/// Address for jump mode.
#[cfg(feature = "jump")]
pub const JUMP_ADDRESS: usize = CONFIG.jump_address as usize;
/// Maximum translation-fence range, in bytes, before selecting a full flush.
pub const TLB_FLUSH_LIMIT: usize = CONFIG.tlb_flush_limit as usize;

/// Allowed next-stage entry ranges for dynamic boot.
#[cfg(not(any(feature = "payload", feature = "jump")))]
pub const DYNAMIC_NEXT_ADDR_RANGE: &NextAddr = &CONFIG.next_addr;
