use sbi_spec::binary::SbiRet;

#[inline]
pub fn handle_ecall(
    function: usize,
    param0: usize,
    param1: usize,
    param2: usize,
    param3: usize,
    param4: usize,
) -> SbiRet {
    use crate::rfence::*;
    use sbi_spec::rfnc::*;
    let hart_mask = crate::HartMask::from_mask_base(param0, param1);
    match function {
        REMOTE_FENCE_I => remote_fence_i(hart_mask),
        REMOTE_SFENCE_VMA => remote_sfence_vma(hart_mask, param2, param3),
        REMOTE_SFENCE_VMA_ASID => remote_sfence_vma_asid(hart_mask, param2, param3, param4),
        REMOTE_HFENCE_GVMA_VMID => remote_hfence_gvma_vmid(hart_mask, param2, param3, param4),
        REMOTE_HFENCE_GVMA => remote_hfence_gvma(hart_mask, param2, param3),
        REMOTE_HFENCE_VVMA_ASID => remote_hfence_vvma_asid(hart_mask, param2, param3, param4),
        REMOTE_HFENCE_VVMA => remote_hfence_vvma(hart_mask, param2, param3),
        _ => SbiRet::not_supported(),
    }
}
