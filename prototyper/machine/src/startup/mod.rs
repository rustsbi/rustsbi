//! Machine startup from raw CPU arrival to typed Rust execution.

mod allocator;
pub(crate) mod contract;
mod from_previous;
mod relocation;
mod runtime;
mod stacks;
mod state;

pub(crate) use runtime::{fail_runtime, publish_runtime};
pub(crate) use stacks::{enter_warm_loop, hart_stack_top};
