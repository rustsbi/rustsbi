//! SBI remote-fence and hypervisor-fence smoke tests.

use core::arch::naked_asm;
use sbi_spec::{
    binary::{HartMask, SbiRet},
    hsm::hart_state,
    pmu::firmware_event,
};
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

    if option_env!("HFENCE_MIXED_H").is_some() {
        test_mixed_h_preflight(hartid, smp);
    }
}

/// SBI HSM state of a started hart.
const STARTED: SbiRet = SbiRet::success(hart_state::STARTED);

/// Size of the parked secondary hart's independent stack.
const MIXED_H_STACK_SIZE: usize = 4096;

/// Independent stack for the secondary hart started by the mixed-H regression.
#[repr(C, align(16))]
struct MixedHSecondaryStack([u8; MIXED_H_STACK_SIZE]);

static MIXED_H_SECONDARY_STACK: MixedHSecondaryStack =
    MixedHSecondaryStack([0; MIXED_H_STACK_SIZE]);

/// Parks the mixed-H regression's secondary hart.
///
/// The hart only needs a valid S-mode stack; it takes no further part in the
/// test and never receives a fence.
#[unsafe(naked)]
unsafe extern "C" fn mixed_h_secondary_entry(_hartid: usize, _opaque: usize) -> ! {
    naked_asm!(
        "csrw sie, zero", // mask S-mode interrupts while parked
        "lla sp, {stack} + {stack_size}",
        "1: wfi",
        "j 1b",
        stack = sym MIXED_H_SECONDARY_STACK,
        stack_size = const MIXED_H_STACK_SIZE,
    )
}

/// Mixed-H regression, enabled at build time with `HFENCE_MIXED_H=1`.
///
/// Starts a second hart whose DTB record lacks the hypervisor extension and
/// requires every HFENCE call targeting it to return `NOT_SUPPORTED` before
/// the calling hart executes anything locally.
///
/// The calling hart may be any hart: QEMU's payload firmware elects the boot
/// hart by a lock race, so this kernel cannot assume hart 0. The firmware
/// keeps the boot hart's hypervisor record through its hardware `misa` probe,
/// while the other hart's record comes from the DTB without `h`.
fn test_mixed_h_preflight(hartid: usize, smp: usize) {
    assert!(
        smp >= 2,
        "the mixed-H HFENCE regression requires at least two harts (smp={smp})"
    );

    // Prove that the calling hart and the firmware really support H: both
    // GVMA calls run the non-full-flush, per-page encoding path, and both
    // VVMA calls run the full-flush path.
    let self_mask = HartMask::from_mask_base(0x1, hartid);
    assert_eq!(
        sbi::remote_hfence_gvma_vmid(self_mask, 0x1000, 0x2000, 0),
        SbiRet::success(0),
        "the calling hart must execute HFENCE.GVMA with VMID"
    );
    assert_eq!(
        sbi::remote_hfence_gvma(self_mask, 0x1000, 0x2000),
        SbiRet::success(0),
        "the calling hart must execute HFENCE.GVMA"
    );
    assert_eq!(
        sbi::remote_hfence_vvma_asid(self_mask, 0, 0, 0),
        SbiRet::success(0),
        "the calling hart must execute HFENCE.VVMA with ASID"
    );
    assert_eq!(
        sbi::remote_hfence_vvma(self_mask, 0, 0),
        SbiRet::success(0),
        "the calling hart must execute HFENCE.VVMA"
    );

    // Start the other hart and park it in its own S-mode loop. The mixed-H
    // DTB hides the hypervisor extension from this hart's feature record,
    // while QEMU still implements the instructions.
    let other = (0..smp)
        .find(|candidate| *candidate != hartid)
        .expect("smp >= 2 guarantees a second hart id");
    assert_eq!(
        sbi::hart_start(other, mixed_h_secondary_entry as *const () as _, 0),
        SbiRet::success(0),
        "hart {other} must start for the mixed-H regression"
    );
    while sbi::hart_get_status(other) != STARTED {
        core::hint::spin_loop();
    }

    let mixed_mask = HartMask::from_mask_base((1 << hartid) | (1 << other), 0);

    // NOT_SUPPORTED rules out a missing check in any target order. The
    // RECEIVED counter also rules out per-target checks that reject only
    // after the calling hart executed; that shape is observable when the
    // calling hart is the first target, which QEMU's boot-hart election
    // makes probabilistic per run.
    crate::pmu::assert_rejected_without_received(firmware_event::HFENCE_GVMA_VMID_RECEIVED, || {
        sbi::remote_hfence_gvma_vmid(mixed_mask, 0x1000, 0x2000, 0)
    });
    crate::pmu::assert_rejected_without_received(firmware_event::HFENCE_GVMA_RECEIVED, || {
        sbi::remote_hfence_gvma(mixed_mask, 0x1000, 0x2000)
    });
    crate::pmu::assert_rejected_without_received(firmware_event::HFENCE_VVMA_ASID_RECEIVED, || {
        sbi::remote_hfence_vvma_asid(mixed_mask, 0, 0, 0)
    });
    crate::pmu::assert_rejected_without_received(firmware_event::HFENCE_VVMA_RECEIVED, || {
        sbi::remote_hfence_vvma(mixed_mask, 0, 0)
    });

    println!("Sbi mixed-H HFENCE preflight test pass");
}
