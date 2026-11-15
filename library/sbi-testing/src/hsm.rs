//! Hart state monitor extension test suite.

use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};
use sbi::{HartMask, SbiRet};
use sbi_spec::hsm::hart_state;

/// Hart state monitor extension test cases.
#[derive(Clone, Debug)]
pub enum Case<'a> {
    /// Can't proceed test for Hart state monitor extension does not exist.
    NotExist,
    /// Test begin.
    Begin,
    /// Test failed for hart started before test begin.
    ///
    /// The returned value includes which hart led to this test failure.
    HartStartedBeforeTest(usize),
    /// Test failed for no other harts are available to be tested.
    NoStoppedHart,
    /// Test process for begin test hart state monitor on one batch.
    BatchBegin(&'a [usize]),
    /// Test process for target hart to be tested has started.
    HartStarted(usize),
    /// Test failed for can't start target hart with [`SbiRet`] error.
    HartStartFailed {
        /// The target hart ID that has failed to start.
        hartid: usize,
        /// The `SbiRet` value for the failed hart start SBI call.
        ret: SbiRet,
    },
    /// Test process for target hart to be tested has non-retentively suspended.
    HartSuspendedNonretentive(usize),
    /// Test process for target hart to be tested has resumed.
    HartResumed(usize),
    /// Test process for target hart to be tested has retentively suspended.
    HartSuspendedRetentive(usize),
    /// Test process for target hart to be tested has stopped.
    HartStopped(usize),
    /// Remote RFence succeeded on a started remote hart.
    RemoteRFencePass(usize),
    /// Remote RFence failed on a started remote hart.
    RemoteRFenceFailed(usize, SbiRet),
    /// Test process for harts on current batch has passed the tests.
    BatchPass(&'a [usize]),
    /// All test cases on hart state monitor module finished.
    Pass,
}

/// Test hart state monitor extension on given harts.
///
/// The test case output is to be handled in `f`.
///
/// # Panics
///
/// Panics when called while another `test_hsm` session is active.
pub fn test(
    primary_hart_id: usize,
    mut hart_mask: usize,
    hart_mask_base: usize,
    mut f: impl FnMut(Case),
) {
    // 不支持 HSM 扩展
    if sbi::probe_extension(sbi::Hsm).is_unavailable() {
        f(Case::NotExist);
        return;
    }
    if SESSION_ACTIVE.swap(true, Ordering::AcqRel) {
        panic!(
            "concurrent or reentrant `test_hsm` call: the previous session \
             may still have secondary harts running on the slots"
        );
    }
    f(Case::Begin);
    let rfnc_available = sbi::probe_extension(sbi::Fence).is_available();

    let mut batch = [0usize; TEST_BATCH_SIZE];
    let mut batch_count = 0;
    let mut batch_size = 0;
    let mut hartid = hart_mask_base;
    while hart_mask != 0 {
        if hartid != primary_hart_id {
            // 副核在测试前必须处于停止状态
            if sbi::hart_get_status(hartid) == STOPPED {
                batch[batch_size] = hartid;
                batch_size += 1;
                // 收集一个批次，执行测试
                if batch_size == TEST_BATCH_SIZE {
                    if test_batch(&batch, rfnc_available, &mut f) {
                        batch_count += 1;
                        batch_size = 0;
                    } else {
                        // Leftover harts may still run on the slots; keep the flag held.
                        return;
                    }
                }
            }
            // 副核不在停止状态
            else {
                f(Case::HartStartedBeforeTest(hartid));
            }
        }
        let distance = hart_mask.trailing_zeros() + 1;
        hart_mask >>= distance;
        hartid += distance as usize;
    }
    // 为不满一批次的核执行测试
    if batch_size > 0 {
        if test_batch(&batch[..batch_size], rfnc_available, &mut f) {
            SESSION_ACTIVE.store(false, Ordering::Release);
            f(Case::Pass);
        }
    }
    // 所有批次通过测试
    else if batch_count > 0 {
        SESSION_ACTIVE.store(false, Ordering::Release);
        f(Case::Pass);
    }
    // 没有找到能参与测试的副核
    else {
        SESSION_ACTIVE.store(false, Ordering::Release);
        f(Case::NoStoppedHart)
    }
}

const STARTED: SbiRet = SbiRet::success(hart_state::STARTED);
const STOPPED: SbiRet = SbiRet::success(hart_state::STOPPED);
const SUSPENDED: SbiRet = SbiRet::success(hart_state::SUSPENDED);

const TEST_BATCH_SIZE: usize = 4;
static STACK: [ItemPerHart; TEST_BATCH_SIZE] = [ItemPerHart::ZERO; TEST_BATCH_SIZE];

/// `true` while a `test_hsm` session owns the slots: acquired at session
/// entry, released only after every hart the session started was observed
/// `STOPPED`.
static SESSION_ACTIVE: AtomicBool = AtomicBool::new(false);

#[repr(C, align(512))]
struct ItemPerHart {
    stage: AtomicU32,
    signal: AtomicU32,
    stack: UnsafeCell<[u8; 504]>,
}

// SAFETY: `SESSION_ACTIVE` serializes sessions, so at most one session's
// harts touch `STACK` at any time. Within a session, `stage` and `signal`
// are atomics, and the `stack` bytes are used only by the one secondary
// hart the slot was handed to via `hart_start`.
unsafe impl Sync for ItemPerHart {}

// The naked entry sets `sp` to `opaque + 512`; pin the slot layout it relies on.
const _: () = assert!(
    core::mem::size_of::<ItemPerHart>() == 512,
    "ItemPerHart must stay a 512-byte slot"
);
const _: () = assert!(
    core::mem::align_of::<ItemPerHart>() == 512,
    "ItemPerHart must stay 512-byte aligned"
);
const _: () = assert!(
    core::mem::offset_of!(ItemPerHart, stack) == 8,
    "stack field must sit at offset 8, ending at the slot top"
);

const STAGE_IDLE: u32 = 0;
const STAGE_STARTED: u32 = 1;
const STAGE_RESUMED: u32 = 2;

impl ItemPerHart {
    #[allow(clippy::declare_interior_mutable_const)]
    const ZERO: Self = Self {
        stage: AtomicU32::new(STAGE_IDLE),
        signal: AtomicU32::new(0),
        stack: UnsafeCell::new([0; 504]),
    };

    #[inline]
    fn reset(&self) -> *const ItemPerHart {
        self.stage.store(STAGE_IDLE, Ordering::Relaxed);
        self as _
    }

    #[inline]
    fn wait_start(&self) {
        while self.stage.load(Ordering::Relaxed) != STAGE_STARTED {
            core::hint::spin_loop();
        }
    }

    #[inline]
    fn wait_resume(&self) {
        while self.stage.load(Ordering::Relaxed) != STAGE_RESUMED {
            core::hint::spin_loop();
        }
    }

    #[inline]
    fn send_signal(&self) {
        self.signal.store(1, Ordering::Release);
    }

    #[inline]
    fn wait_signal(&self) {
        while self
            .signal
            .compare_exchange(1, 0, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
    }
}

/// 测试一批核
fn test_batch(batch: &[usize], rfnc_available: bool, mut f: impl FnMut(Case)) -> bool {
    f(Case::BatchBegin(batch));
    // 初始这些核都是停止状态，测试 start
    for (i, hartid) in batch.iter().copied().enumerate() {
        let ptr = STACK[i].reset();
        let ret = sbi::hart_start(hartid, test_entry as *const () as _, ptr as _);
        if ret.is_err() {
            f(Case::HartStartFailed { hartid, ret });
            return false;
        }
    }
    // 测试不可恢复休眠
    for (i, hartid) in batch.iter().copied().enumerate() {
        let item = &STACK[i];
        // 等待完成启动
        while sbi::hart_get_status(hartid) != STARTED {
            core::hint::spin_loop();
        }
        f(Case::HartStarted(hartid));
        if rfnc_available {
            let rfence_ret = sbi::remote_fence_i(HartMask::from_mask_base(1, hartid));
            if rfence_ret.is_ok() {
                f(Case::RemoteRFencePass(hartid));
            } else {
                f(Case::RemoteRFenceFailed(hartid, rfence_ret));
            }
        }
        // 等待信号
        item.wait_start();
        // 发出信号
        item.send_signal();
        // 等待完成休眠
        while sbi::hart_get_status(hartid) != SUSPENDED {
            core::hint::spin_loop();
        }
        f(Case::HartSuspendedNonretentive(hartid));
    }
    // 全部唤醒
    let mut mask = 1usize;
    for hartid in &batch[1..] {
        mask |= 1 << (hartid - batch[0]);
    }
    sbi::send_ipi(HartMask::from_mask_base(mask, batch[0]));
    // 测试可恢复休眠
    for (i, hartid) in batch.iter().copied().enumerate() {
        let item = &STACK[i];
        // 等待完成恢复
        while sbi::hart_get_status(hartid) != STARTED {
            core::hint::spin_loop();
        }
        f(Case::HartResumed(hartid));
        // 等待信号
        item.wait_resume();
        // 发出信号
        item.send_signal();
        // 等待完成休眠
        while sbi::hart_get_status(hartid) != SUSPENDED {
            core::hint::spin_loop();
        }
        f(Case::HartSuspendedRetentive(hartid));
        // 单独恢复
        sbi::send_ipi(HartMask::from_mask_base(1, hartid));
        // 等待关闭
        while sbi::hart_get_status(hartid) != STOPPED {
            core::hint::spin_loop();
        }
        f(Case::HartStopped(hartid));
    }
    f(Case::BatchPass(batch));
    true
}

/// 测试用启动入口
#[unsafe(naked)]
unsafe extern "C" fn test_entry(hartid: usize, opaque: *const ItemPerHart) -> ! {
    core::arch::naked_asm!(
        "csrw sie, zero",   // 关中断
        "call {set_stack}", // 设置栈
        "j    {rust_main}", // 进入 rust
        set_stack = sym set_stack,
        rust_main = sym rust_main,
    )
}

#[unsafe(naked)]
unsafe extern "C" fn set_stack(hart_id: usize, ptr: *const ItemPerHart) {
    core::arch::naked_asm!("addi sp, a1, 512", "ret");
}

/// # Safety
///
/// The caller must ensure that `opaque`:
/// - is the address of a `STACK` slot returned by `ItemPerHart::reset()`,
///   handed to this hart by `sbi::hart_start` and passed back to the same
///   entry on non-retentive resume, and
/// - is only referenced through shared references across harts.
#[inline(never)]
unsafe extern "C" fn rust_main(hart_id: usize, opaque: *const ItemPerHart) -> ! {
    // SAFETY: `opaque` fulfills the `# Safety` contract above.
    let item = unsafe { &*opaque };
    match item.stage.compare_exchange(
        STAGE_IDLE,
        STAGE_STARTED,
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => {
            item.wait_signal();
            let ret =
                sbi::hart_suspend(sbi::NonRetentive, test_entry as *const () as _, opaque as _);
            unreachable!("suspend [{hart_id}] but {ret:?}")
        }
        Err(STAGE_STARTED) => {
            item.stage.store(STAGE_RESUMED, Ordering::Release);
            item.wait_signal();
            let _ = sbi::hart_suspend(sbi::Retentive, test_entry as *const () as _, opaque as _);
            let ret = sbi::hart_stop();
            unreachable!("suspend [{hart_id}] but {ret:?}")
        }
        Err(_) => unreachable!(),
    }
}
