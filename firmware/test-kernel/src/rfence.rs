//! SBI remote-fence and hypervisor-fence smoke tests.

use sbi_spec::binary::{HartMask, SbiRet};
use sbi_testing::sbi;

/// Exercises the RFENCE calls for the current hart and checks an invalid
/// hart ID for the calls whose target validation is observable.
pub(crate) fn test(hartid: usize, smp: usize) {
    let self_mask = HartMask::from_mask_base(0x1, hartid);
    let invalid_hart = smp + 1;

    // A stopped hart remains assigned to the supervisor. It can be skipped
    // without rejecting an otherwise valid IPI or remote-fence mask.
    for target in 0..smp {
        if target == hartid
            || sbi::hart_get_status(target) != SbiRet::success(sbi_spec::hsm::hart_state::STOPPED)
        {
            continue;
        }
        let stopped = HartMask::from_mask_base(1, target);
        assert_eq!(sbi::send_ipi(stopped), SbiRet::success(0));
        let mixed = HartMask::from_mask_base((1 << hartid) | (1 << target), 0);
        for mask in [stopped, mixed] {
            assert_eq!(sbi::remote_fence_i(mask), SbiRet::success(0));
            assert_eq!(sbi::remote_sfence_vma(mask, 0, 0), SbiRet::success(0));
            assert_eq!(
                sbi::remote_sfence_vma_asid(mask, 0, 0, 0),
                SbiRet::success(0)
            );
        }
        println!("Sbi stopped-hart IPI/RFENCE test pass: hart {target}");
    }

    // Fence.i is allowed to be unsupported by a platform.
    let ret = sbi::remote_fence_i(self_mask);
    assert!(ret.is_ok() || ret == SbiRet::not_supported());
    let ret = sbi::remote_fence_i(HartMask::from_mask_base(0x1, invalid_hart));
    assert_eq!(ret, SbiRet::invalid_param());

    // SFence.vma is allowed to be unsupported by a platform.
    let ret = sbi::remote_sfence_vma(self_mask, 0, 0);
    assert!(ret.is_ok() || ret == SbiRet::not_supported());
    let ret = sbi::remote_sfence_vma(HartMask::from_mask_base(0x1, invalid_hart), 0, 0);
    assert_eq!(ret, SbiRet::invalid_param());

    let ret = sbi::remote_sfence_vma_asid(self_mask, 0, 0, 0);
    assert!(ret.is_ok() || ret == SbiRet::not_supported());

    // Hypervisor fences are optional and therefore accept NOT_SUPPORTED.
    let ret = sbi::remote_hfence_gvma(self_mask, 0, 0);
    assert!(ret.is_ok() || ret == SbiRet::not_supported());
    let ret = sbi::remote_hfence_gvma_vmid(self_mask, 0, 0, 0);
    assert!(ret.is_ok() || ret == SbiRet::not_supported());
    let ret = sbi::remote_hfence_vvma(self_mask, 0, 0);
    assert!(ret.is_ok() || ret == SbiRet::not_supported());
    let ret = sbi::remote_hfence_vvma_asid(self_mask, 0, 0, 0);
    assert!(ret.is_ok() || ret == SbiRet::not_supported());

    println!("[34m[ INFO] Sbi `RFNC` test pass[0m");
}
