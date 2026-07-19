//! Limits generated for the selected RISC-V firmware image.

pub(crate) const SBI_LINK_START_ADDRESS: usize = 0x8000_0000;

static_toml::static_toml! {
    static CONFIG = include_toml!("../../target/config.toml");
}

pub(crate) const BOOT_DTB_MAX_SIZE: usize = CONFIG.boot_dtb_max_size as usize;
pub(crate) const BOOT_STACK_SIZE: usize = CONFIG.stack_size_per_hart as usize;
pub(crate) const HEAP_SIZE: usize = CONFIG.heap_size as usize;
pub(crate) const HART_CAPACITY: usize = CONFIG.num_hart_max as usize;
pub(crate) const TRAP_STACK_SIZE: usize = CONFIG.stack_size_per_hart as usize;
pub(crate) const TRUSTED_TARGET: bool = CONFIG.trusted_target;

pub(crate) fn next_address_allowed(address: usize) -> bool {
    CONFIG.next_addr.iter().any(|range| {
        range.start < range.end && (range.start as usize..range.end as usize).contains(&address)
    })
}

#[cfg(feature = "jump")]
pub(crate) const FIXED_NEXT_ADDRESS: usize = CONFIG.jump_address as usize;
