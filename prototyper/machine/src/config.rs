//! Private static capacities for the initial machine runtime.

pub(crate) const SBI_LINK_START_ADDRESS: usize = 0x8000_0000;
pub(crate) const BOOT_DTB_MAX_SIZE: usize = 0x40000;
pub(crate) const BOOT_STACK_SIZE: usize = 0x4000;
pub(crate) const HEAP_SIZE: usize = 0x80000;
pub(crate) const HART_CAPACITY: usize = 8;
pub(crate) const TRAP_STACK_SIZE: usize = 0x4000;
pub(crate) const TRUSTED_TARGET: bool = false;

pub(crate) fn next_address_allowed(address: usize) -> bool {
    crate::entry::image::next_address_allowed(address)
}
