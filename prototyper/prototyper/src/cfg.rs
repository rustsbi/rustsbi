use static_toml::static_toml;

/// The address where the SBI link start.
pub const SBI_LINK_START_ADDRESS: usize = 0x80000000;

static_toml! {
    const CONFIG = include_toml!("../../target/config.toml");
}

#[cfg(not(any(feature = "payload", feature = "jump")))]
pub type NextAddr = crate::cfg::config::next_addr::NextAddr;

/// Maximum number of supported harts.
pub const NUM_HART_MAX: usize = CONFIG.num_hart_max as usize;
/// Minimum trap stack size enforced for `cfg(debug_assertions)` builds.
///
/// Without optimisation, recursive serde-device-tree visitors and the
/// per-hart init code each consume 1.4-2.0 KiB per frame; the 16 KiB
/// release default is not enough to absorb that, so debug builds floor
/// the configured value at 64 KiB.
#[cfg(debug_assertions)]
pub const DEBUG_STACK_FLOOR: usize = 64 * 1024;

/// Returns the larger of `configured` and the debug-only stack floor.
#[cfg(debug_assertions)]
const fn apply_debug_stack_floor(configured: usize) -> usize {
    if configured > DEBUG_STACK_FLOOR {
        configured
    } else {
        DEBUG_STACK_FLOOR
    }
}

/// Stack size per hart (hardware thread) in bytes.
#[cfg(not(debug_assertions))]
pub const STACK_SIZE_PER_HART: usize = CONFIG.stack_size_per_hart as usize;
/// Stack size per hart in bytes; debug builds raise the configured value
/// to a 64 KiB floor to absorb the larger un-optimized stack frames.
#[cfg(debug_assertions)]
pub const STACK_SIZE_PER_HART: usize = apply_debug_stack_floor(CONFIG.stack_size_per_hart as usize);

// Compile-time guarantee: debug builds always have at least the floor.
#[cfg(debug_assertions)]
const _: () = assert!(STACK_SIZE_PER_HART >= DEBUG_STACK_FLOOR);
// Compile-time guarantee: the floor preserves any larger configured value.
#[cfg(debug_assertions)]
const _: () = {
    assert!(apply_debug_stack_floor(0) == DEBUG_STACK_FLOOR);
    assert!(apply_debug_stack_floor(DEBUG_STACK_FLOOR) == DEBUG_STACK_FLOOR);
    assert!(apply_debug_stack_floor(DEBUG_STACK_FLOOR + 1) == DEBUG_STACK_FLOOR + 1);
    assert!(apply_debug_stack_floor(usize::MAX) == usize::MAX);
};
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

/// The dynamic valid next addr ranges.
#[cfg(not(any(feature = "payload", feature = "jump")))]
pub const DYNAMIC_NEXT_ADDR_RANGE: &NextAddr = &CONFIG.next_addr;
