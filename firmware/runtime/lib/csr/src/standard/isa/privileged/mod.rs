//! Privileged register definitions and specialized hardware operations.

pub use riscv_csr_spec::standard::isa::privileged::{debug, supervisor, virtual_supervisor};

pub mod hypervisor;
pub mod machine;
