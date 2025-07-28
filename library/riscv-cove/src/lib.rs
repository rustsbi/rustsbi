//! Confidential VM Extension (CoVE) structure and constant definitions.
//!
//! Confidential VM Extension (CoVE) provides an interface for a scalable
//! Trusted Execution Environment (TEE) that supports hardware virtual-machine-based
//! workloads on RISC-V platforms.
//!
//! This crate can be integrated as part of RustSBI and used in Prototyper,
//! or included as a component of Rust-based bare-metal applications or operating
//! systems to facilitate invoking services provided by the Confidential VM Extension.
#![no_std]

// §10
pub mod host;
// §11
pub mod interrupt;
// §12
pub mod guest;

/// Converts SBI EID from str.
const fn eid_from_str(name: &str) -> i32 {
    match *name.as_bytes() {
        [a] => i32::from_be_bytes([0, 0, 0, a]),
        [a, b] => i32::from_be_bytes([0, 0, a, b]),
        [a, b, c] => i32::from_be_bytes([0, a, b, c]),
        [a, b, c, d] => i32::from_be_bytes([a, b, c, d]),
        _ => unreachable!(),
    }
}
#[cfg(test)]
mod tests {
    use static_assertions::const_assert_eq;
    // §10
    #[test]
    fn test_cove_host() {
        use crate::host::*;
        const_assert_eq!(0x434F5648, EID_COVH);
        const_assert_eq!(0, GET_TSM_INFO);
        const_assert_eq!(1, CONVERT_PAGES);
        const_assert_eq!(2, RECLAIM_PAGES);
        const_assert_eq!(3, GLOBAL_FENCE);
        const_assert_eq!(4, LOCAL_FENCE);
        const_assert_eq!(5, CREATE_TVM);
        const_assert_eq!(6, FINALIZE_TVM);
        const_assert_eq!(8, DESTROY_TVM);
        const_assert_eq!(9, ADD_TVM_MEMORY_REGION);
        const_assert_eq!(10, ADD_TVM_PAGE_TABLE_PAGES);
        const_assert_eq!(11, ADD_TVM_MEASURED_PAGES);
        const_assert_eq!(12, ADD_TVM_ZERO_PAGES);
        const_assert_eq!(13, ADD_TVM_SHARED_PAGES);
        const_assert_eq!(14, CREATE_TVM_VCPU);
        const_assert_eq!(15, RUN_TVM_VCPU);
        const_assert_eq!(16, TVM_FENCE);
        const_assert_eq!(17, TVM_INVALIDATE_PAGES);
        const_assert_eq!(18, TVM_VALIDATE_PAGES);
        const_assert_eq!(19, TVM_REMOVE_PAGES);
    }

    // §11
    #[test]
    fn test_cove_interrupt() {
        use crate::interrupt::*;
        const_assert_eq!(0x434F5649, EID_COVI);
        const_assert_eq!(0, INIT_TVM_AIA);
        const_assert_eq!(1, SET_TVM_AIA_CPU_IMSIC_ADDR);
        const_assert_eq!(2, CONVERT_AIA_IMSIC);
        const_assert_eq!(3, RECLAIM_TVM_AIA_IMSIC);
        const_assert_eq!(4, BIND_AIA_IMSIC);
        const_assert_eq!(5, UNBIND_AIA_IMSIC_BEGIN);
        const_assert_eq!(6, UNBIND_AIA_IMSIC_END);
        const_assert_eq!(7, INJECT_TVM_CPU);
        const_assert_eq!(8, REBIND_AIA_IMSIC_BEGIN);
        const_assert_eq!(9, REBIND_AIA_IMSIC_CLONE);
        const_assert_eq!(10, REBIND_AIA_IMSIC_END);
    }

    // §12
    #[test]
    fn test_cove_guest() {
        use crate::guest::*;
        const_assert_eq!(0x434F5647, EID_COVG);
        const_assert_eq!(0, ADD_MMIO_REGION);
        const_assert_eq!(1, REMOVE_MMIO_REGION);
        const_assert_eq!(2, SHARE_MEMORY_REGION);
        const_assert_eq!(3, UNSHARE_MEMORY_REGION);
        const_assert_eq!(4, ALLOW_EXTERNAL_INTERRUPT);
        const_assert_eq!(5, DENY_EXTERNAL_INTERRUPT);
        const_assert_eq!(6, GET_ATTESTATION_CAPABILITIES);
        const_assert_eq!(7, EXTEND_MEASUREMENT);
        const_assert_eq!(8, GET_EVIDENCE);
        const_assert_eq!(9, RETRIEVE_SECRET);
        const_assert_eq!(10, READ_MEASUREMENT);
    }
}
