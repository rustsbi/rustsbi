//! Chapter 10, COVE Host Extension (EID #0x434F5648 "COVH").
use riscv_cove::host::{
    ADD_TVM_MEASURED_PAGES, ADD_TVM_MEMORY_REGION, ADD_TVM_PAGE_TABLE_PAGES, ADD_TVM_SHARED_PAGES,
    ADD_TVM_ZERO_PAGES, CONVERT_PAGES, CREATE_TVM, CREATE_TVM_VCPU, DESTROY_TVM, EID_COVH,
    FINALIZE_TVM, GET_TSM_INFO, GLOBAL_FENCE, LOCAL_FENCE, PROMOTE_TO_TVM, RECLAIM_PAGES,
    RUN_TVM_VCPU, TVM_FENCE, TVM_INVALIDATE_PAGES, TVM_REMOVE_PAGES, TVM_VALIDATE_PAGES, TsmInfo,
    TsmPageType, TvmCreateParams,
};
use sbi_spec::binary::{Physical, SbiRet};

/// Read the current TSM state, its configuration and supported features.
#[inline]
#[doc(alias = "sbi_covh_get_tsm_info")]
pub fn covh_get_tsm_info(mem: Physical<&mut TsmInfo>) -> SbiRet {
    // TODO mem.phys_addr_hi should be used. The physical memory parameter should be parsed in `_hi` and `_lo` pairs, ref: https://lists.riscv.org/g/tech-ap-tee/topic/114646239#msg207 .
    unsafe { sbi_rt::raw::sbi_call_2(EID_COVH, GET_TSM_INFO, mem.phys_addr_lo(), mem.num_bytes()) }
}

/// Convert non-confidential memory pages to confidential-memory.
#[inline]
#[doc(alias = "sbi_covh_convert_pages")]
pub fn covh_convert_pages(base_page_addr: usize, num_pages: usize) -> SbiRet {
    unsafe { sbi_rt::raw::sbi_call_2(EID_COVH, CONVERT_PAGES, base_page_addr, num_pages) }
}

/// Reclaim confidential memory pages.
#[inline]
#[doc(alias = "sbi_covh_reclaim_pages")]
pub fn covh_reclaim_pages(base_page_addr: usize, num_pages: usize) -> SbiRet {
    unsafe { sbi_rt::raw::sbi_call_2(EID_COVH, RECLAIM_PAGES, base_page_addr, num_pages) }
}

/// Initiate a TLB invalidation sequence for all pages marked for conversion.
#[inline]
#[doc(alias = "sbi_covh_global_fence")]
pub fn covh_global_fence() -> SbiRet {
    unsafe { sbi_rt::raw::sbi_call_0(EID_COVH, GLOBAL_FENCE) }
}

/// Invalidate TLB entries for all pages pending conversion by an in-progress
/// TLB invalidation operation on the local CPU.
#[inline]
#[doc(alias = "sbi_covh_local_fence")]
pub fn covh_local_fence() -> SbiRet {
    unsafe { sbi_rt::raw::sbi_call_0(EID_COVH, LOCAL_FENCE) }
}

/// Create a confidential TVM using the specified parameters.
#[inline]
#[doc(alias = "sbi_covh_create_tvm")]
pub fn covh_create_tvm(mem: Physical<&mut TvmCreateParams>) -> SbiRet {
    unsafe { sbi_rt::raw::sbi_call_2(EID_COVH, CREATE_TVM, mem.phys_addr_lo(), mem.num_bytes()) }
}

/// Transition the TVM from the `TVM_INITIALIZING` state to a `TVM_RUNNABLE` state.
// TODO: identity_addr points to a 64-bytes buffer containing a host-defined TVM identity.
#[inline]
#[doc(alias = "sbi_covh_finalize_tvm")]
pub fn covh_finalize_tvm(
    guest_id: usize,
    entry_spec: usize,
    entry_arg: usize,
    identity_addr: usize,
) -> SbiRet {
    unsafe {
        sbi_rt::raw::sbi_call_4(
            EID_COVH,
            FINALIZE_TVM,
            guest_id,
            entry_spec,
            entry_arg,
            identity_addr,
        )
    }
}

/// Promote a VM to a TVM by the host.
// TODO: identity_addr points to a 64-bytes buffer containing a host-defined TVM identity.
#[inline]
#[doc(alias = "sbi_covh_promote_to_tvm")]
pub fn covh_promote_to_tvm(
    fdt: Physical<&u64>,
    tap: Physical<&u64>,
    entry_spec: usize,
    identity_addr: usize,
) -> SbiRet {
    unsafe {
        sbi_rt::raw::sbi_call_4(
            EID_COVH,
            PROMOTE_TO_TVM,
            fdt.phys_addr_lo(),
            tap.phys_addr_lo(),
            entry_spec,
            identity_addr,
        )
    }
}

/// Destroy a confidential TVM.
#[inline]
#[doc(alias = "sbi_covh_destory_tvm")]
pub fn covh_destory_tvm(guest_id: usize) -> SbiRet {
    unsafe { sbi_rt::raw::sbi_call_1(EID_COVH, DESTROY_TVM, guest_id) }
}

/// Mark the range of TVM physical address space as reserved for the mapping of confidential memory.
#[inline]
#[doc(alias = "sbi_covh_add_tvm_memory_region")]
pub fn covh_add_tvm_memory_region(guest_id: usize, gpa: Physical<&u8>) -> SbiRet {
    unsafe {
        sbi_rt::raw::sbi_call_3(
            EID_COVH,
            ADD_TVM_MEMORY_REGION,
            guest_id,
            gpa.phys_addr_lo(),
            gpa.num_bytes(),
        )
    }
}

/// Add confidential memory pages to the TVM’s page-table page pool.
#[inline]
#[doc(alias = "sbi_covh_add_tvm_page_table_pages")]
pub fn covh_add_tvm_page_table_pages(
    guest_id: usize,
    base_page_addr: usize,
    num_pages: usize,
) -> SbiRet {
    unsafe {
        sbi_rt::raw::sbi_call_3(
            EID_COVH,
            ADD_TVM_PAGE_TABLE_PAGES,
            guest_id,
            base_page_addr,
            num_pages,
        )
    }
}

/// Copy pages from non-confidential memory to confidential memory,
/// then measures and maps the pages at the TVM physical address space.
#[inline]
#[doc(alias = "sbi_covh_add_tvm_measured_pages")]
pub fn covh_add_tvm_measured_pages(
    guest_id: usize,
    src_addr: usize,
    dst_addr: usize,
    page_type: TsmPageType,
    num_pages: usize,
    guest_gpa: usize,
) -> SbiRet {
    unsafe {
        sbi_rt::raw::sbi_call_6(
            EID_COVH,
            ADD_TVM_MEASURED_PAGES,
            guest_id,
            src_addr,
            dst_addr,
            page_type as usize,
            num_pages,
            guest_gpa,
        )
    }
}

/// Map zero-filled pages of confidential memory into the TVM' s physical address space.
#[inline]
#[doc(alias = "sbi_covh_add_tvm_zero_pages")]
pub fn covh_add_tvm_zero_pages(
    guest_id: usize,
    base_page_addr: usize,
    page_type: TsmPageType,
    num_pages: usize,
    tvm_page_addr: usize,
) -> SbiRet {
    unsafe {
        sbi_rt::raw::sbi_call_5(
            EID_COVH,
            ADD_TVM_ZERO_PAGES,
            guest_id,
            base_page_addr,
            page_type as usize,
            num_pages,
            tvm_page_addr,
        )
    }
}

/// Map non-confidential memory pages into the TVM' s physical address space.
#[inline]
#[doc(alias = "sbi_covh_add_tvm_shared_pages")]
pub fn covh_add_tvm_shared_pages(
    guest_id: usize,
    base_page_addr: usize,
    page_type: usize,
    num_pages: usize,
    tvm_page_addr: usize,
) -> SbiRet {
    unsafe {
        sbi_rt::raw::sbi_call_5(
            EID_COVH,
            ADD_TVM_SHARED_PAGES,
            guest_id,
            base_page_addr,
            page_type as usize,
            num_pages,
            tvm_page_addr,
        )
    }
}

/// Add a vCPU to the TVM.
#[inline]
#[doc(alias = "sbi_covh_create_tvm_vcpu")]
pub fn covh_create_tvm_vcpu(guest_id: usize, vcpu_id: usize, state_page_addr: usize) -> SbiRet {
    unsafe {
        sbi_rt::raw::sbi_call_3(
            EID_COVH,
            CREATE_TVM_VCPU,
            guest_id,
            vcpu_id,
            state_page_addr,
        )
    }
}

/// Run the cCPU in the TVM.
#[inline]
#[doc(alias = "sbi_covh_run_tvm_vcpu")]
pub fn covh_run_tvm_vcpu(guest_id: usize, vcpu_id: usize) -> SbiRet {
    unsafe { sbi_rt::raw::sbi_call_2(EID_COVH, RUN_TVM_VCPU, guest_id, vcpu_id) }
}

/// Initiate a TLB invalidation sequence for all pages that
/// have been invalidated in the given TVM’s address space.
#[inline]
#[doc(alias = "sbi_covh_tvm_fence")]
pub fn covh_tvm_fence(guest_id: usize) -> SbiRet {
    unsafe { sbi_rt::raw::sbi_call_1(EID_COVH, TVM_FENCE, guest_id) }
}

/// Invalidate the pages in the specified range of guest physical address
/// space and thus marks the pages as blocked from any further TVM accesses.
#[inline]
#[doc(alias = "sbi_covh_tvm_invalidate_pages")]
pub fn covh_tvm_invalidate_pages(guest_id: usize, gpa: usize, length: usize) -> SbiRet {
    unsafe { sbi_rt::raw::sbi_call_3(EID_COVH, TVM_INVALIDATE_PAGES, guest_id, gpa, length) }
}

/// Mark the invalidated pages in the specified range
/// of guest physical address space as present.
#[inline]
#[doc(alias = "sbi_covh_tvm_validate_pages")]
pub fn covh_tvm_validate_pages(guest_id: usize, gpa: usize, length: usize) -> SbiRet {
    unsafe { sbi_rt::raw::sbi_call_3(EID_COVH, TVM_VALIDATE_PAGES, guest_id, gpa, length) }
}

/// Removes mappings for invalidated pages in the
/// specified range of guest physical address space.
#[inline]
#[doc(alias = "sbi_covh_tvm_remove_pages")]
pub fn covh_tvm_remove_pages(guest_id: usize, gpa: usize, length: usize) -> SbiRet {
    unsafe { sbi_rt::raw::sbi_call_3(EID_COVH, TVM_REMOVE_PAGES, guest_id, gpa, length) }
}
