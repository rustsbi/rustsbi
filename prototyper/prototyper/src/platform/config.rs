//! Compile-time bounds for boot-local platform discovery.

use static_toml::static_toml;

static_toml! {
    const CONFIG = include_toml!("../../target/config.toml");
}

pub(super) const NUM_HART_MAX: usize = CONFIG.num_hart_max as usize;
pub(super) const BOOT_DTB_MAX_SIZE: usize = CONFIG.boot_dtb_max_size as usize;
pub(super) const BOOT_DTB_MAX_DEPTH: usize = CONFIG.boot_dtb_max_depth as usize;
pub(super) const BOOT_DTB_MAX_NODES: usize = CONFIG.boot_dtb_max_nodes as usize;
pub(super) const BOOT_DTB_MAX_PROPERTIES: usize = CONFIG.boot_dtb_max_properties as usize;
pub(super) const BOOT_DTB_MAX_EDITS: usize = CONFIG.boot_dtb_max_edits as usize;
