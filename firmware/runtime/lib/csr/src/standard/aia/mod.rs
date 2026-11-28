//! Advanced Interrupt Architecture definitions and hardware access.

pub mod imsic;

pub use imsic::{Eidelivery, Eithreshold, Iid, ImsicRegister, TopExternalInterrupt};
pub use riscv_csr_spec::standard::aia::{
    MajorIid, TopInterrupt, hypervisor, interrupt, machine, supervisor, virtual_supervisor,
};
