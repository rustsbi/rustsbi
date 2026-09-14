//! Firmware Runtime build configuration.
//!
//! These constants size the Runtime-owned per-hart facilities (trap stacks,
//! per-hart state). They are Runtime configuration, not policy inputs:
//! policy code discovers which harts exist; only the Runtime decides how
//! many harts it has storage for and how large each stack is.

/// Maximum number of harts supported by the Runtime's per-hart facilities.
///
/// A hart whose `mhartid` is at or above this bound cannot initialize trap
/// handling and fail-stops during entry.
pub const NUM_HART_MAX: usize = 8;

/// Per-hart stack size in bytes.
///
/// One stack is sequentially reused for boot and traps.
/// The value must keep every slot 16-byte aligned and leave room for the
/// private trap frame plus the configured Runtime/policy call depth.
pub const STACK_SIZE_PER_HART: usize = 16 * 1024;

const _: () = assert!(STACK_SIZE_PER_HART.is_multiple_of(16));
