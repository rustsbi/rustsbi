//! Chapter 15. Nested Acceleration Extension (EID #0x4E41434C "NACL")

use crate::binary::{sbi_call_0, sbi_call_1, sbi_call_3};

use sbi_spec::{
    binary::{SbiRet, SharedPtr},
    nacl::{shmem_size, EID_NACL, PROBE_FEATURE, SET_SHMEM, SYNC_CSR, SYNC_HFENCE, SYNC_SRET},
};

/// Probe a nested acceleration feature.
///
/// This is a mandatory function of the SBI nested acceleration extension.
///
/// # Parameters
///
/// The `feature_id` parameter specifies the nested acceleration feature to probe.
/// Possible feature IDs are defined in the table below:
///
/// # Return value
///
/// This function always returns `SbiRet::success()` in `SbiRet.error`.
/// It returns 0 in `SbiRet.value` if the given `feature_id` is not available,
/// or 1 in `SbiRet.value` if it is available.
///
/// This function is defined in RISC-V SBI Specification chapter 15.5.
#[inline]
pub fn nacl_probe_feature(feature_id: u32) -> SbiRet {
    sbi_call_1(EID_NACL, PROBE_FEATURE, feature_id as _)
}

/// Set and enable the shared memory for nested acceleration on the calling hart.
///
/// This is a mandatory function of the SBI nested acceleration extension.
///
/// # Parameters
///
/// If `shmem` parameter is not all-ones bitwise, then `shmem` specifies the shared
/// memory physical base address. `shmem` MUST be 4096 bytes (i.e., page) aligned, and
/// the size of the shared memory must be `4096 + (XLEN * 128)` bytes.
///
/// If `shmem` parameter is all-ones bitwise, then the nested acceleration features
/// are disabled.
///
/// The `flags` parameter is reserved for future use and must be zero.
///
/// The possible error codes returned in `SbiRet.error` are shown in the table below:
///
/// | Error code                  | Description
/// |:----------------------------|:---------------------------------
/// | `SbiRet::success()`         | Shared memory was set or cleared successfully.
/// | `SbiRet::invalid_param()`   | The `flags` parameter is not zero or or the `shmem` parameter is not 4096 bytes aligned.
/// | `SbiRet::invalid_address()` | The shared memory pointed to by the `shmem` parameters does not satisfy the requirements.
///
/// This function is defined in RISC-V SBI Specification chapter 15.6.
#[inline]
pub fn nacl_set_shmem(shmem: SharedPtr<[u8; shmem_size::NATIVE]>, flags: usize) -> SbiRet {
    sbi_call_3(
        EID_NACL,
        SET_SHMEM,
        shmem.phys_addr_lo(),
        shmem.phys_addr_hi(),
        flags,
    )
}

/// Synchronize CSRs in the nested acceleration shared memory.
///
/// This is an optional function that is only available if the SBI_NACL_FEAT_SYNC_CSR feature is available.
///
/// # Parameters
///
/// The parameter `csr_num` specifies the set of RISC-V H-extension CSRs to be synchronized.
///
/// If `csr_num` is all-ones bitwise, then all RISC-V H-extension CSRs implemented by the SBI implementation (or L0 hypervisor) are synchronized.
///
/// If `(csr_num & 0x300) == 0x200` and `csr_num < 0x1000` then only a single
/// RISC-V H-extension CSR specified by the csr_num parameter is synchronized.
///
/// # Return value
///
/// The possible error codes returned in `SbiRet.error` are shown in the table below:
///
/// | Error code                | Description
/// |:--------------------------|:---------------------------------
/// | `SbiRet::success()`       | CSRs synchronized successfully.
/// | `SbiRet::not_supported()` | SBI_NACL_FEAT_SYNC_CSR feature is not available.
/// | `SbiRet::invalid_param()` | `csr_num` is not all-ones bitwise and either: <br> * `(csr_num & 0x300) != 0x200` or <br> * `csr_num >= 0x1000` or <br> * `csr_num` is not implemented by the SBI implementation
/// | `SbiRet::no_shmem()`      | Nested acceleration shared memory not available.
///
/// This function is defined in RISC-V SBI Specification chapter 15.7.
#[inline]
pub fn nacl_sync_csr(csr_num: usize) -> SbiRet {
    sbi_call_1(EID_NACL, SYNC_CSR, csr_num)
}

/// Synchronize HFENCEs in the nested acceleration shared memory.
///
/// This is an optional function that is only available if the SBI_NACL_FEAT_SYNC_HFENCE feature is available.
///
/// # Parameters
///
/// The parameter `entry_index` specifies the set of nested HFENCE entries to be synchronized.
///
/// If `entry_index` is all-ones bitwise, then all nested HFENCE entries are synchronized.
///
/// If `entry_index < (3840 / XLEN)` then only a single nested HFENCE entry specified by the `entry_index` parameter is synchronized
///
/// # Return value
///
/// The possible error codes returned in `SbiRet.error` are shown in the table below:
///
/// | Error code                | Description
/// |:--------------------------|:---------------------------------
/// | `SbiRet::success()`       | HFENCEs synchronized successfully.
/// | `SbiRet::not_supported()` | SBI_NACL_FEAT_SYNC_HFENCE feature is not available.
/// | `SbiRet::invalid_param()` | `entry_index` is not all-ones bitwise and `entry_index >= (3840 / XLEN)`.
/// | `SbiRet::no_shmem()`      | Nested acceleration shared memory not available.
///
/// This function is defined in RISC-V SBI Specification chapter 15.8.
#[inline]
pub fn nacl_sync_hfence(entry_index: usize) -> SbiRet {
    sbi_call_1(EID_NACL, SYNC_HFENCE, entry_index)
}

/// Synchronize CSRs and HFENCEs in the NACL shared memory and emulate the SRET instruction.
///
/// This is an optional function that is only available if the SBI_NACL_FEAT_SYNC_SRET feature is available.
///
/// This function is used by supervisor software (or L1 hypervisor) to do a synchronizing SRET request,
/// and the SBI implementation (or L0 hypervisor) MUST handle it.
///
/// # Return value
///
/// This function does not return upon success, and the possible error codes
/// returned in `SbiRet.error` upon failure are shown in the table below:
///
/// | Error code                | Description
/// |:--------------------------|:------------
/// | `SbiRet::no_shmem()`      | Nested acceleration shared memory not available.
/// | `SbiRet::not_supported()` | SBI_NACL_FEAT_SYNC_SRET feature is not available.
///
/// This function is defined in RISC-V SBI Specification chapter 15.9.
#[inline]
pub fn nacl_sync_sret() -> SbiRet {
    sbi_call_0(EID_NACL, SYNC_SRET)
}
