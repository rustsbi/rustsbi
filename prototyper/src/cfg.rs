/// Maximum number of supported harts.
pub const NUM_HART_MAX: usize = 8;
/// Stack size per hart (hardware thread) in bytes.
pub const LEN_STACK_PER_HART: usize = 16 * 1024;
/// Page size
pub const PAGE_SIZE: usize = 4096;
/// TLB_FLUSH_LIMIT defines the TLB refresh range limit. 
/// If the TLB refresh range is greater than TLB_FLUSH_LIMIT, the entire TLB is refreshed.
pub const TLB_FLUSH_LIMIT: usize = 4 * PAGE_SIZE;


#[cfg(feature = "jump")]
pub const JUMP_ADDRESS: usize = 0x80200000;
