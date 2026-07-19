//! Bounded storage sizes for host-side validators and ownership tests.

pub(crate) const BOOT_DTB_MAX_SIZE: usize = 0x40000;
pub(crate) const HART_CAPACITY: usize = 8;
pub(crate) const TRAP_STACK_SIZE: usize = 16 * 1024;
pub(crate) const TRUSTED_TARGET: bool = false;

pub(crate) fn next_address_allowed(address: usize) -> bool {
    (0x2000_0000..0x2400_0000).contains(&address) || (0x8000_0000..0x9000_0000).contains(&address)
}
