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
