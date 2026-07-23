//! Compile-time bounds for boot-local platform discovery.

pub(super) const NUM_HART_MAX: usize = 8;
pub(super) const BOOT_DTB_MAX_SIZE: usize = 0x40000;
pub(super) const BOOT_DTB_MAX_DEPTH: usize = 64;
pub(super) const BOOT_DTB_MAX_NODES: usize = 4096;
pub(super) const BOOT_DTB_MAX_PROPERTIES: usize = 16384;
pub(super) const BOOT_DTB_MAX_EDITS: usize = 256;
