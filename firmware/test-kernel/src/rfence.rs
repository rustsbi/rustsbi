//! SBI remote-fence and hypervisor-fence smoke tests.

use core::{
    arch::naked_asm,
    sync::atomic::{AtomicBool, Ordering},
};
use sbi_spec::{
    binary::{HartMask, SbiRet},
    hsm::hart_state,
};
use sbi_testing::sbi;

/// Exercises the RFENCE calls for the current hart and checks an invalid
/// hart ID for the calls whose target validation is observable.
pub(crate) fn test(hartid: usize, smp: usize, frequency: u64) {
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

    if let Some(peer) = (0..smp).find(|&hart| hart != hartid) {
        test_mutual_requests(hartid, peer, frequency);
    }
    println!("[34m[ INFO] Sbi `RFNC` test pass[0m");
}

static READY: AtomicBool = AtomicBool::new(false);
static DONE: AtomicBool = AtomicBool::new(false);
static RELEASE: AtomicBool = AtomicBool::new(false);

#[repr(align(16))]
struct Stack {
    _bytes: [u8; 16384],
}
static mut STACK: Stack = Stack { _bytes: [0; 16384] };

fn test_mutual_requests(current: usize, peer: usize, frequency: u64) {
    assert_eq!(
        sbi::hart_get_status(peer),
        SbiRet::success(hart_state::STOPPED)
    );
    assert_eq!(
        sbi::hart_start(peer, peer_entry as *const () as usize, current),
        SbiRet::success(0)
    );
    wait(frequency, || READY.load(Ordering::Acquire));
    send_requests(peer);
    wait(frequency, || DONE.load(Ordering::Acquire));
    RELEASE.store(true, Ordering::Release);
    wait(frequency, || {
        sbi::hart_get_status(peer) == SbiRet::success(hart_state::STOPPED)
    });
    println!("[rfence] simultaneous requests in both directions pass");
}

fn send_requests(peer: usize) {
    let target = HartMask::from_mask_base(1, peer);
    for _ in 0..64 {
        assert_eq!(sbi::remote_fence_i(target), SbiRet::success(0));
        assert_eq!(sbi::remote_sfence_vma(target, 0, 0), SbiRet::success(0));
    }
}

fn wait(frequency: u64, mut condition: impl FnMut() -> bool) {
    let start = riscv::register::time::read64();
    while !condition() {
        assert!(
            riscv::register::time::read64().wrapping_sub(start) < frequency * 5,
            "mutual RFENCE test timed out"
        );
        core::hint::spin_loop();
    }
}

/// # Safety
/// Only the peer selected by test_mutual_requests may use STACK. It must stop
/// before the test returns or starts another peer on this stack.
#[unsafe(naked)]
unsafe extern "C" fn peer_entry(_hart: usize, _source: usize) -> ! {
    naked_asm!(
        "csrw sie, zero",
        "mv tp, a0",
        "lla sp, {stack} + {size}",
        "j {worker}",
        stack = sym STACK,
        size = const core::mem::size_of::<Stack>(),
        worker = sym peer_worker,
    );
}

extern "C" fn peer_worker(_hart: usize, source: usize) -> ! {
    READY.store(true, Ordering::Release);
    send_requests(source);
    DONE.store(true, Ordering::Release);
    // Stay started until the source has finished sending requests to us.
    while !RELEASE.load(Ordering::Acquire) {
        core::hint::spin_loop();
    }
    let ret = sbi::hart_stop();
    panic!("RFENCE peer stop returned: {ret:?}");
}
