//! Penglai PMP Extension structure and constant definitions.
//!
//! Penglai PMP Extension is a lightweight TEE solution built on RISC-V’s PMP feature.
//! This crate provides the SBI structures and constant definitions required by
//! the Penglai PMP extension.
//!
//! This crate can be integrated as part of RustSBI and used in Prototyper,
//! or included as a component of Rust-based bare-metal applications or operating systems
//! to facilitate invoking services provided by the Penglai PMP extension.
#![no_std]

pub mod enclave;
pub mod host;

#[cfg(test)]
mod tests {
    use static_assertions::const_assert_eq;

    #[test]
    fn test_penglai_host() {
        use crate::host::*;
        const_assert_eq!(0x100100, EID_PENGLAI_HOST);
        const_assert_eq!(100, MM_INIT);
        const_assert_eq!(99, CREATE_ENCLAVE);
        const_assert_eq!(98, ATTEST_ENCLAVE);
        const_assert_eq!(97, RUN_ENCLAVE);
        const_assert_eq!(96, STOP_ENCLAVE);
        const_assert_eq!(95, RESUME_ENCLAVE);
        const_assert_eq!(94, DESTROY_ENCLAVE);
        const_assert_eq!(93, ALLOC_ENCLAVE_MM);
        const_assert_eq!(92, MEMORY_EXTEND);
        const_assert_eq!(91, MEMORY_RECLAIM);
        const_assert_eq!(90, FREE_ENCLAVE_MEM);
        const_assert_eq!(88, DEBUG_PRINT);

        const_assert_eq!(2, resume_status::RESUME_FROM_OCALL);
        const_assert_eq!(2003, resume_status::RESUME_FROM_STOP);
        const_assert_eq!(2000, resume_status::RESUME_FROM_TIMER_IRQ);
    }

    #[test]
    fn test_penglai_enclave() {
        use crate::enclave::*;
        const_assert_eq!(0x100101, EID_PENGLAI_ENCLAVE);
        const_assert_eq!(99, ENCLAVE_EXIT);
        const_assert_eq!(98, ENCLAVE_OCALL);
        const_assert_eq!(88, GET_KEY);

        const_assert_eq!(3, ocall_type::OCALL_SYS_WRITE);
        const_assert_eq!(9, ocall_type::OCALL_USER_DEFINED);
    }
}
