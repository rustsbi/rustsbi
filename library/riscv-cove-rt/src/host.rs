//! Chapter 10, COVE Host Extension (EID #0x434F5648 "COVH").
use riscv_cove::host::{CONVERT_PAGES, EID_COVH, GET_TSM_INFO, RECLAIM_PAGES, TsmInfo};
use sbi_spec::binary::{Physical, SbiRet};

/// Reads the current TSM state, its configuration and supported features.
#[inline]
#[doc(alias = "sbi_covh_get_tsm_info")]
pub fn covh_get_tsm_info(mem: Physical<&mut TsmInfo>) -> SbiRet {
    // TODO mem.phys_addr_hi should be used. The physical memory parameter should be parsed in `_hi` and `_lo` pairs, ref: https://lists.riscv.org/g/tech-ap-tee/topic/114646239#msg207 .
    unsafe { sbi_rt::raw::sbi_call_2(EID_COVH, GET_TSM_INFO, mem.phys_addr_lo(), mem.num_bytes()) }
}

/// Convert normal physical pages to confidential memory pages for CoVE TVM usage.
#[inline]
#[doc(alias = "sbi_covh_convert_pages")]
pub fn covh_convert_pages(base_page_addr: usize, num_pages: usize) -> SbiRet {
    unsafe { sbi_rt::raw::sbi_call_2(EID_COVH, CONVERT_PAGES, base_page_addr, num_pages) }
}

/// Reclaim confidential memory pages back to normal physical pages.
#[inline]
#[doc(alias = "sbi_covh_reclaim_pages")]
pub fn covh_reclaim_pages(page_addr: usize, num_pages: usize) -> SbiRet {
    unsafe { sbi_rt::raw::sbi_call_2(EID_COVH, RECLAIM_PAGES, page_addr, num_pages) }
}

// TODO other functions
/// Add initial zeroed confidential pages to a TVM (demand-mapped, not measured)
///
/// # Parameters
/// - `tvm_id`: TVM identifier
/// - `page_addr`: Confidential physical page address (already converted)
/// - `page_type`: Page attributes/type
/// - `num_pages`: Number of 4-KiB pages to add
/// - `tvm_page_addr`: TVM GPA where the pages will be mapped
///
/// Corresponds to `sbi_covh_add_tvm_zero_pages` (FID #12) defined in the COVE Host
/// Extension (EID: `0x434F5648`).
///
/// # Errors
/// - `SBI_ERR_INVALID_PARAM` — invalid parameters
/// - `SBI_ERR_INVALID_ADDRESS` — invalid physical or TVM address
/// - `SBI_ERR_DENIED` — operation denied by TSM
/// - `SBI_ERR_NOT_SUPPORTED` — TSM does not support this operation
/// - `SBI_ERR_FAILED` — generic failure
#[inline]
#[doc(alias = "sbi_covh_add_tvm_zero_pages")]
pub fn covh_add_tvm_zero_pages(
    tvm_id: usize,
    page_addr: usize,
    page_type: usize,
    num_pages: usize,
    tvm_page_addr: usize,
) -> SbiRet {
    unsafe {
        sbi_rt::raw::sbi_call_5(
            EID_COVH,
            ADD_TVM_ZERO_PAGES,
            tvm_id,
            page_addr,
            page_type,
            num_pages,
            tvm_page_addr,
        )
    }
}

/// Map non-confidential shared pages into a TVM (host and TVM both accessible)
///
/// # Parameters
/// - `tvm_id`: TVM identifier
/// - `page_addr`: Non-confidential physical page address (host-accessible)
/// - `page_type`: Page attributes/type
/// - `num_pages`: Number of 4-KiB pages to add
/// - `tvm_page_addr`: TVM GPA where the pages will be mapped
///
/// Corresponds to `sbi_covh_add_tvm_shared_pages` (FID #13) defined in the COVE Host
/// Extension (EID: `0x434F5648`).
///
/// Note: Shared pages are non-confidential (not protected by TSM) and do NOT
/// participate in measurement; the TVM must declare use of shared memory via
/// the COVG extension as appropriate.
///
/// # Errors
/// - `SBI_ERR_INVALID_PARAM` — invalid parameters
/// - `SBI_ERR_INVALID_ADDRESS` — invalid physical or TVM address
/// - `SBI_ERR_DENIED` — operation denied
/// - `SBI_ERR_NOT_SUPPORTED` — TSM or platform doesn't support shared pages
/// - `SBI_ERR_FAILED` — generic failure
#[inline]
#[doc(alias = "sbi_covh_add_tvm_shared_pages")]
pub fn covh_add_tvm_shared_pages(
    tvm_id: usize,
    page_addr: usize,
    page_type: usize,
    num_pages: usize,
    tvm_page_addr: usize,
) -> SbiRet {
    unsafe {
        sbi_rt::raw::sbi_call_5(
            EID_COVH,
            ADD_TVM_SHARED_PAGES,
            tvm_id,
            page_addr,
            page_type,
            num_pages,
            tvm_page_addr,
        )
    }
}

/// Create a vCPU inside a TVM.
///
/// # Parameters
/// - `tvm_id`: TVM identifier
/// - `tvm_vcpu_id`: Physical address of `TvmVcpuCreateParams` structure
/// - `tvm_state_page_addr`: Physical address of pages to store vCPU state
///
/// Corresponds to `sbi_covh_create_tvm_vcpu` (FID #14) defined in the COVE Host
/// Extension (EID: `0x434F5648`).
///
/// # Errors
/// - `SBI_ERR_INVALID_PARAM` — invalid parameters
/// - `SBI_ERR_INVALID_ADDRESS` — invalid physical address
/// - `SBI_ERR_DENIED` — operation denied by TSM
/// - `SBI_ERR_NOT_SUPPORTED` — vCPU creation not supported
/// - `SBI_ERR_FAILED` — generic failure
#[inline]
#[doc(alias = "sbi_covh_create_tvm_vcpu")]
pub fn covh_create_tvm_vcpu(
    tvm_id: usize,
    tvm_vcpu_id: usize,
    tvm_state_page_addr: usize,
) -> SbiRet {
    unsafe {
        sbi_rt::raw::sbi_call_3(
            EID_COVH,
            CREATE_TVM_VCPU,
            tvm_id,
            tvm_vcpu_id,
            tvm_state_page_addr,
        )
    }
}

/// Run a TVM vCPU until it exits; returned `SbiRet.value` contains the TVM exit reason.
///
/// # Parameters
/// - `tvm_id`: TVM identifier
/// - `vcpu_id`: vCPU identifier
///
/// Corresponds to `sbi_covh_run_tvm_vcpu` (FID #15) defined in the COVE Host
/// Extension (EID: `0x434F5648`).
///
/// # Errors / Return
/// - On success, `SbiRet.error` is `SBI_SUCCESS` and `SbiRet.value` contains the
///   TVM exit reason (e.g., ECALL, load/store page fault, etc.).
/// - On failure, standard SBI error codes may be returned:
///   `SBI_ERR_INVALID_PARAM`, `SBI_ERR_DENIED`, `SBI_ERR_NOT_SUPPORTED`,
///   `SBI_ERR_FAILED`.
///
/// Note: After return the host MUST inspect the guest `scause` and the NACL
/// shared memory to determine the precise exit cause and decide the next steps.
#[inline]
#[doc(alias = "sbi_covh_run_tvm_vcpu")]
pub fn covh_run_tvm_vcpu(tvm_id: usize, vcpu_id: usize) -> SbiRet {
    unsafe { sbi_rt::raw::sbi_call_2(EID_COVH, RUN_TVM_VCPU, tvm_id, vcpu_id) }
}
