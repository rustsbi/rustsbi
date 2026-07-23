//! Machine startup from raw CPU arrival to typed Rust execution.

mod allocator;
mod raw;
mod relocation;
mod runtime;
mod stacks;
mod state;

pub use raw::raw_entry;
pub(crate) use runtime::{fail_runtime, publish_runtime};
pub(crate) use stacks::{enter_warm_loop, hart_stack_top};
