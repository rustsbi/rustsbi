use static_toml::static_toml;

/// The address where the SBI link start.
pub const SBI_LINK_START_ADDRESS: usize = 0x80000000;

static_toml! {
    const CONFIG = include_toml!("../../target/config.toml");
}

/// Maximum number of supported harts.
pub const NUM_HART_MAX: usize = CONFIG.num_hart_max as usize;
/// Stack size per hart (hardware thread) in bytes.
pub const LEN_STACK_PER_HART: usize = CONFIG.len_stack_per_hart as usize;
/// Heap Size of SBI firmware.
pub const HEAP_SIZE: usize = CONFIG.heap_size as usize;
/// Platform page size.
pub const PAGE_SIZE: usize = CONFIG.page_size as usize;
/// Log Level.
pub const LOG_LEVEL: &'static str = CONFIG.log_level;
/// Address for jump mode.
#[cfg(feature = "jump")]
pub const JUMP_ADDRESS: usize = CONFIG.jump_address as usize;
/// TLB_FLUSH_LIMIT defines the TLB refresh range limit.
/// If the TLB refresh range is greater than TLB_FLUSH_LIMIT, the entire TLB is refreshed.
pub const TLB_FLUSH_LIMIT: usize = CONFIG.tlb_flush_limit as usize;
