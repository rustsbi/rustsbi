//! Hart protocol model and orchestration tests.

use super::*;
use crate::hart::fence::RemoteFenceRequest;
#[cfg(feature = "hypervisor")]
use crate::hart::fence::targets_support_request;
use crate::hart::start::StartHandshake;
use crate::{HartTargets, NextStage};

fn started<const N: usize>() -> HartAdmissionState<N> {
    HartAdmissionState::new(
        core::array::from_fn(|index| index),
        [HartState::Started; N],
        [true; N],
    )
    .unwrap()
}

#[test]
fn hart_state_allows_only_the_defined_success_edges() {
    let mut start = HartAdmissionState::new([0], [HartState::Stopped], [true]).unwrap();
    start.begin_start(0).unwrap();
    assert_eq!(start.state(0), Ok(HartState::StartPending));
    assert_eq!(start.begin_start(0), Err(AdmissionError::InvalidTransition));
    start.complete_start(0).unwrap();

    let mut stop = started::<1>();
    stop.begin_stop(0).unwrap();
    stop.finish_stop(0, &ClaimedWork::default()).unwrap();

    let mut suspend = started::<1>();
    suspend.begin_suspend(0).unwrap();
    suspend.finish_suspend(0).unwrap();
    suspend.begin_resume(0).unwrap();
    suspend.finish_resume(0).unwrap();
}

#[test]
fn only_the_target_can_publish_started_after_prepared_and_proceed() {
    let mut work = HartAdmissionState::new([0], [HartState::Stopped], [true]).unwrap();
    let mut handshake = StartHandshake::default();
    work.begin_start(0).unwrap();
    assert_eq!(work.state(0), Ok(HartState::StartPending));
    handshake.publish_prepared().unwrap();
    assert_eq!(work.state(0), Ok(HartState::StartPending));
    handshake.source_proceed().unwrap();
    assert_eq!(work.state(0), Ok(HartState::StartPending));
    handshake.target_consume().unwrap();
    work.complete_start(0).unwrap();
    assert_eq!(work.state(0), Ok(HartState::Started));

    let mut failed = StartHandshake::default();
    let mut stopped = HartAdmissionState::new([0], [HartState::Stopped], [true]).unwrap();
    stopped.begin_start(0).unwrap();
    failed.publish_failed().unwrap();
    failed.source_observed_failure().unwrap();
    stopped.cancel_start(0).unwrap();
    assert_eq!(stopped.state(0), Ok(HartState::Stopped));
}

#[test]
fn failed_multi_target_commit_changes_nothing() {
    let mut work = HartAdmissionState::new(
        [0, 1, 2],
        [HartState::Started, HartState::Stopped, HartState::Started],
        [true; 3],
    )
    .unwrap();
    let before = work.clone();
    let mut targets = HartSet::singleton(0).unwrap();
    targets.insert(1).unwrap();
    assert_eq!(
        work.commit_rfence(2, targets, RemoteFenceRequest::FenceI),
        Err(AdmissionError::Unavailable)
    );
    assert_eq!(work, before);
    assert!(work.invariants_hold(&[ClaimedWork::default(); 3]));
}

#[test]
fn sparse_physical_targets_resolve_inside_one_lifecycle_snapshot() {
    let work = HartAdmissionState::new(
        [0, 8, 0x1000],
        [HartState::Started, HartState::Stopped, HartState::Suspended],
        [true, true, false],
    )
    .unwrap();
    assert_eq!(
        work.resolve_targets(HartTargets::selected(1, 8)),
        Err(AdmissionError::Unavailable)
    );
    assert_eq!(
        work.resolve_targets(HartTargets::selected(1, 9)),
        Err(AdmissionError::InvalidHart)
    );
    assert_eq!(
        work.resolve_targets(HartTargets::all_available()),
        HartSet::singleton(0)
    );
    assert_eq!(
        work.resolve_targets(HartTargets::selected(0, usize::MAX)),
        Ok(HartSet::empty())
    );
}

#[test]
fn stop_gate_drains_the_pre_gate_finite_batch_before_stopped() {
    let mut work = started::<2>();
    let target = HartSet::singleton(1).unwrap();
    work.commit_ipi(target).unwrap();
    work.commit_rfence(0, target, RemoteFenceRequest::FenceI)
        .unwrap();
    work.begin_stop(1).unwrap();
    assert_eq!(work.commit_ipi(target), Err(AdmissionError::Unavailable));
    let mut batches = [ClaimedWork::default(); 2];
    work.claim(1, &mut batches[1]).unwrap();
    batches[1].supervisor_ipi = false;
    work.complete(1, 0, &mut batches[1]).unwrap();
    work.retire(0).unwrap();
    work.finish_stop(1, &batches[1]).unwrap();
    assert_eq!(work.state(1), Ok(HartState::Stopped));
    assert!(work.invariants_hold(&batches));
}

#[test]
fn suspend_cannot_publish_sleep_while_accepted_work_is_pending() {
    let mut work = started::<1>();
    let target = HartSet::singleton(0).unwrap();
    work.commit_ipi(target).unwrap();
    work.begin_suspend(0).unwrap();
    assert_eq!(work.finish_suspend(0), Err(AdmissionError::MissingRelation));
    assert_eq!(work.state(0), Ok(HartState::SuspendPending));

    let mut batch = ClaimedWork::default();
    work.claim(0, &mut batch).unwrap();
    batch.supervisor_ipi = false;
    assert!(batch.is_empty());
    work.finish_suspend(0).unwrap();
    assert_eq!(work.state(0), Ok(HartState::Suspended));
}

#[test]
fn system_suspend_checks_all_peers_at_the_transition_commit() {
    let mut work = started::<3>();
    let before = work.clone();
    assert_eq!(
        work.begin_system_suspend(0),
        Err(AdmissionError::Unavailable)
    );
    assert_eq!(work, before);

    work.begin_stop(1).unwrap();
    work.finish_stop(1, &ClaimedWork::default()).unwrap();
    work.begin_stop(2).unwrap();
    work.finish_stop(2, &ClaimedWork::default()).unwrap();
    work.begin_system_suspend(0).unwrap();
    assert_eq!(work.state(0), Ok(HartState::SuspendPending));
    assert_eq!(work.state(1), Ok(HartState::Stopped));
    assert_eq!(work.state(2), Ok(HartState::Stopped));
}

#[test]
fn resume_edges_are_closed_around_the_suspended_state() {
    let mut work = started::<1>();
    assert_eq!(work.begin_resume(0), Err(AdmissionError::InvalidTransition));
    work.begin_suspend(0).unwrap();
    work.finish_suspend(0).unwrap();
    work.begin_resume(0).unwrap();
    assert_eq!(work.state(0), Ok(HartState::ResumePending));
    assert_eq!(work.begin_resume(0), Err(AdmissionError::InvalidTransition));
    work.finish_resume(0).unwrap();
    assert_eq!(work.state(0), Ok(HartState::Started));
}

#[test]
fn complete_claim_owns_a_finite_snapshot_and_retires_once() {
    let mut work = started::<3>();
    let mut targets = HartSet::singleton(1).unwrap();
    targets.insert(2).unwrap();
    work.commit_rfence(0, targets, RemoteFenceRequest::FenceI)
        .unwrap();
    let mut batches = [ClaimedWork::default(); 3];
    work.claim(1, &mut batches[1]).unwrap();
    assert_eq!(
        work.copy_request(1, 0, &batches[1]),
        Ok(RemoteFenceRequest::FenceI)
    );
    work.complete(1, 0, &mut batches[1]).unwrap();
    assert_eq!(work.retire(0), Err(AdmissionError::MissingRelation));
    work.claim(2, &mut batches[2]).unwrap();
    work.complete(2, 0, &mut batches[2]).unwrap();
    assert_eq!(work.retire(0), Ok(RemoteFenceRequest::FenceI));
    assert_eq!(work.retire(0), Err(AdmissionError::MissingRelation));
    assert!(work.invariants_hold(&batches));
}

#[test]
fn resume_finishes_at_the_capture_cut_despite_later_arrival() {
    let mut work = started::<3>();
    work.begin_suspend(1).unwrap();
    work.finish_suspend(1).unwrap();
    work.commit_rfence(
        0,
        HartSet::singleton(1).unwrap(),
        RemoteFenceRequest::FenceI,
    )
    .unwrap();
    work.begin_resume(1).unwrap();
    let mut batches = [ClaimedWork::default(); 3];
    work.claim(1, &mut batches[1]).unwrap();
    work.commit_rfence(
        2,
        HartSet::singleton(1).unwrap(),
        RemoteFenceRequest::SfenceVma { start: 0, size: 0 },
    )
    .unwrap();
    work.complete(1, 0, &mut batches[1]).unwrap();
    work.retire(0).unwrap();
    work.finish_resume(1).unwrap();
    assert_eq!(work.state(1), Ok(HartState::Started));
    assert!(work.invariants_hold(&batches));
}

#[test]
fn simultaneous_sources_remain_distinct_and_active_storage_is_immutable() {
    let mut work = started::<3>();
    let target = HartSet::singleton(2).unwrap();
    let first = RemoteFenceRequest::SfenceVma {
        start: 0x1000,
        size: 0x2000,
    };
    let second = RemoteFenceRequest::SfenceVmaAsid {
        start: 0x1000,
        size: 0x2000,
        asid: 7,
    };
    work.commit_rfence(0, target, first).unwrap();
    work.commit_rfence(1, target, second).unwrap();
    assert_eq!(
        work.commit_rfence(0, target, RemoteFenceRequest::FenceI),
        Err(AdmissionError::SourceBusy)
    );
    let mut batches = [ClaimedWork::default(); 3];
    work.claim(2, &mut batches[2]).unwrap();
    assert_eq!(
        batches[2].sources.iter().collect::<alloc::vec::Vec<_>>(),
        [0, 1]
    );
    assert_eq!(work.copy_request(2, 0, &batches[2]), Ok(first));
    assert_eq!(work.copy_request(2, 1, &batches[2]), Ok(second));
    work.complete(2, 0, &mut batches[2]).unwrap();
    work.retire(0).unwrap();
    assert_eq!(work.copy_request(2, 1, &batches[2]), Ok(second));
    work.complete(2, 1, &mut batches[2]).unwrap();
    work.retire(1).unwrap();
    assert!(work.invariants_hold(&batches));
}

#[test]
fn a_claimed_high_source_cannot_be_overtaken_by_low_source_reissue() {
    let mut work = started::<3>();
    let target = HartSet::singleton(2).unwrap();
    work.commit_rfence(0, target, RemoteFenceRequest::FenceI)
        .unwrap();
    work.commit_rfence(1, target, RemoteFenceRequest::FenceI)
        .unwrap();
    let mut batches = [ClaimedWork::default(); 3];
    work.claim(2, &mut batches[2]).unwrap();
    work.complete(2, 0, &mut batches[2]).unwrap();
    work.retire(0).unwrap();
    work.commit_rfence(0, target, RemoteFenceRequest::FenceI)
        .unwrap();
    assert!(batches[2].sources.contains(1));
    assert!(!batches[2].sources.contains(0));
    assert!(work.fence_pending(2, 0));
    work.complete(2, 1, &mut batches[2]).unwrap();
    work.retire(1).unwrap();
    assert!(work.invariants_hold(&batches));
}

#[derive(Default)]
struct DelayedIpi {
    accepted: HartSet,
    delivered: HartSet,
    prepared: bool,
}

impl DelayedIpi {
    fn prepare(&mut self, succeeds: bool) -> Result<(), ()> {
        if succeeds {
            self.prepared = true;
            Ok(())
        } else {
            Err(())
        }
    }

    fn notify(&mut self, targets: HartSet) {
        assert!(self.prepared);
        self.accepted.0 |= targets.0;
    }

    fn deliver(&mut self) {
        self.delivered.0 |= self.accepted.0;
        self.accepted = HartSet::empty();
    }

    fn claim(&mut self, target: usize) {
        self.delivered.remove(target);
    }
}

#[test]
fn delayed_ring_and_spurious_handler_cannot_lose_committed_work() {
    let targets = HartSet::singleton(1).unwrap();
    let mut failed_device = DelayedIpi::default();
    let mut untouched = started::<2>();
    let before = untouched.clone();
    assert_eq!(failed_device.prepare(false), Err(()));
    assert_eq!(untouched, before);

    let mut device = DelayedIpi::default();
    device.prepare(true).unwrap();
    untouched.commit_ipi(targets).unwrap();
    device.notify(targets);
    let mut batches = [ClaimedWork::default(); 2];
    // A spurious handler before physical delivery still consumes the
    // authoritative software level. The later delayed ring is harmless.
    untouched.claim(1, &mut batches[1]).unwrap();
    assert!(batches[1].supervisor_ipi);
    batches[1].supervisor_ipi = false;
    device.deliver();
    device.claim(1);
    untouched.claim(1, &mut batches[1]).unwrap();
    assert!(batches[1].is_empty());
    assert!(untouched.invariants_hold(&batches));
}

#[test]
fn ordinary_ipi_coalesces_before_claim_and_reappears_during_drain() {
    let mut work = started::<2>();
    let target = HartSet::singleton(1).unwrap();
    work.commit_ipi(target).unwrap();
    work.commit_ipi(target).unwrap();
    let mut batches = [ClaimedWork::default(); 2];
    work.claim(1, &mut batches[1]).unwrap();
    assert!(batches[1].supervisor_ipi);
    work.commit_ipi(target).unwrap();
    batches[1].supervisor_ipi = false;
    work.claim(1, &mut batches[1]).unwrap();
    assert!(batches[1].supervisor_ipi);
    batches[1].supervisor_ipi = false;
    assert!(work.invariants_hold(&batches));
}

#[derive(Clone, Copy)]
enum MutualAction {
    Commit0,
    Commit1,
    Claim0,
    Claim1,
    Complete0,
    Complete1,
}

#[test]
fn explores_mutual_rfence_schedules_without_a_wait_cycle() {
    let work = started::<2>();
    let batches = [ClaimedWork::default(); 2];
    let actions = [
        MutualAction::Commit0,
        MutualAction::Commit1,
        MutualAction::Claim0,
        MutualAction::Claim1,
        MutualAction::Complete0,
        MutualAction::Complete1,
    ];
    let mut terminals = 0;
    explore(work, batches, &actions, 0, &mut terminals);
    assert!(terminals >= 4);
}

#[test]
fn explores_three_hart_cycle_claim_and_completion_schedules() {
    let mut work = started::<3>();
    work.commit_rfence(
        0,
        HartSet::singleton(1).unwrap(),
        RemoteFenceRequest::FenceI,
    )
    .unwrap();
    work.commit_rfence(
        1,
        HartSet::singleton(2).unwrap(),
        RemoteFenceRequest::FenceI,
    )
    .unwrap();
    work.commit_rfence(
        2,
        HartSet::singleton(0).unwrap(),
        RemoteFenceRequest::FenceI,
    )
    .unwrap();
    let mut terminals = 0;
    explore_cycle(work, [ClaimedWork::default(); 3], 0, &mut terminals);
    assert!(terminals >= 6);
}

fn explore_cycle(
    work: HartAdmissionState<3>,
    batches: [ClaimedWork; 3],
    used: u8,
    terminals: &mut usize,
) {
    assert!(work.invariants_hold(&batches));
    if used == 0b11_1111 {
        assert!(work.all_fence_sources_idle());
        *terminals += 1;
        return;
    }
    for action in 0..6 {
        if used & (1 << action) != 0 {
            continue;
        }
        let mut next_work = work.clone();
        let mut next_batches = batches;
        let result = match action {
            0..=2 => next_work.claim(action, &mut next_batches[action]),
            3 => next_work
                .copy_request(0, 2, &next_batches[0])
                .and_then(|_| next_work.complete(0, 2, &mut next_batches[0]))
                .and_then(|_| next_work.retire(2).map(|_| ())),
            4 => next_work
                .copy_request(1, 0, &next_batches[1])
                .and_then(|_| next_work.complete(1, 0, &mut next_batches[1]))
                .and_then(|_| next_work.retire(0).map(|_| ())),
            5 => next_work
                .copy_request(2, 1, &next_batches[2])
                .and_then(|_| next_work.complete(2, 1, &mut next_batches[2]))
                .and_then(|_| next_work.retire(1).map(|_| ())),
            _ => unreachable!(),
        };
        if result.is_ok() {
            explore_cycle(next_work, next_batches, used | (1 << action), terminals);
        }
    }
}

fn explore(
    work: HartAdmissionState<2>,
    batches: [ClaimedWork; 2],
    actions: &[MutualAction; 6],
    used: u8,
    terminals: &mut usize,
) {
    assert!(work.invariants_hold(&batches));
    if used == 0b11_1111 {
        assert!(work.all_fence_sources_idle());
        *terminals += 1;
        return;
    }
    for (index, action) in actions.iter().enumerate() {
        if used & (1 << index) != 0 {
            continue;
        }
        let mut next_work = work.clone();
        let mut next_batches = batches;
        let result = match action {
            MutualAction::Commit0 => next_work.commit_rfence(
                0,
                HartSet::singleton(1).unwrap(),
                RemoteFenceRequest::FenceI,
            ),
            MutualAction::Commit1 => next_work.commit_rfence(
                1,
                HartSet::singleton(0).unwrap(),
                RemoteFenceRequest::FenceI,
            ),
            MutualAction::Claim0 => next_work.claim(0, &mut next_batches[0]),
            MutualAction::Claim1 => next_work.claim(1, &mut next_batches[1]),
            MutualAction::Complete0 => next_work
                .copy_request(0, 1, &next_batches[0])
                .and_then(|_| next_work.complete(0, 1, &mut next_batches[0]))
                .and_then(|_| next_work.retire(1).map(|_| ())),
            MutualAction::Complete1 => next_work
                .copy_request(1, 0, &next_batches[1])
                .and_then(|_| next_work.complete(1, 0, &mut next_batches[1]))
                .and_then(|_| next_work.retire(0).map(|_| ())),
        };
        if result.is_ok() {
            explore(
                next_work,
                next_batches,
                actions,
                used | (1 << index),
                terminals,
            );
        }
    }
}

#[derive(Default)]
struct RecordedIpi {
    notified: std::sync::Mutex<alloc::vec::Vec<usize>>,
    claimed: std::sync::Mutex<alloc::vec::Vec<usize>>,
    notification_count: AtomicUsize,
}

impl IpiDevice for RecordedIpi {
    fn notify(&self, hart_id: usize) {
        self.notified.lock().unwrap().push(hart_id);
        self.notification_count.fetch_add(1, Ordering::Release);
    }

    fn claim(&self, hart_id: usize) {
        self.claimed.lock().unwrap().push(hart_id);
    }
}

struct BlockingIpi {
    entered: std::sync::mpsc::Sender<()>,
    release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
}

impl IpiDevice for BlockingIpi {
    fn notify(&self, _hart_id: usize) {
        self.entered.send(()).unwrap();
        self.release.lock().unwrap().recv().unwrap();
    }

    fn claim(&self, _hart_id: usize) {}
}

fn two_hart_admission(device: Arc<RecordedIpi>) -> Arc<HartAdmission> {
    let admission = HartAdmission::new(device, &[0, 8], 0, &[true, true]).unwrap();
    {
        let mut state = admission.state.lock();
        state.begin_start(1).unwrap();
        state.complete_start(1).unwrap();
    }
    admission
}

#[test]
fn admission_commits_before_ring_and_claims_by_physical_id() {
    let device = Arc::new(RecordedIpi::default());
    let admission = two_hart_admission(device.clone());

    admission.send(HartTargets::selected(1, 8)).unwrap();
    assert_eq!(*device.notified.lock().unwrap(), [8]);
    {
        let state = admission.state.lock();
        assert!(state.ipi_pending(1));
    }

    admission.drain(8, true).unwrap();
    assert_eq!(*device.claimed.lock().unwrap(), [8]);
    let state = admission.state.lock();
    assert!(!state.ipi_pending(1));
    assert!(!state.fence_pending(1, 0));
}

#[test]
fn device_notification_does_not_hold_the_protocol_lock() {
    use std::time::Duration;

    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let device = Arc::new(BlockingIpi {
        entered: entered_tx,
        release: std::sync::Mutex::new(release_rx),
    });
    let admission = HartAdmission::new(device, &[0, 8], 0, &[true, true]).unwrap();
    {
        let mut state = admission.state.lock();
        state.begin_start(1).unwrap();
        state.complete_start(1).unwrap();
    }

    let sender = admission.clone();
    let send = std::thread::spawn(move || sender.send(HartTargets::selected(1, 8)));
    entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();

    let observer = admission.clone();
    let (status_tx, status_rx) = std::sync::mpsc::channel();
    let status = std::thread::spawn(move || status_tx.send(observer.status(0)).unwrap());
    let observed = status_rx.recv_timeout(Duration::from_secs(1));

    release_tx.send(()).unwrap();
    status.join().unwrap();
    assert_eq!(send.join().unwrap(), Ok(()));
    assert_eq!(observed.unwrap(), Ok(HartState::Started));
}

#[test]
fn terminal_notification_rings_peers_without_protocol_state() {
    let device = Arc::new(RecordedIpi::default());
    let admission = two_hart_admission(device.clone());

    admission.notify_terminal_peers();
    assert_eq!(*device.notified.lock().unwrap(), [8]);
    let state = admission.state.lock();
    assert!(!state.ipi_pending(0));
    assert!(!state.ipi_pending(1));
}

#[test]
fn admission_rejects_suspend_without_a_constructed_ipi_wake_path() {
    let device = Arc::new(RecordedIpi::default());
    let admission = HartAdmission::new(device, &[0], 0, &[false]).unwrap();

    assert_eq!(admission.suspend_current(), Err(HartError::NotSupported));
    assert_eq!(admission.status(0), Ok(HartState::Started));
}

#[test]
fn pre_gate_ipi_makes_retentive_suspend_resume_immediately() {
    let device = Arc::new(RecordedIpi::default());
    let admission = HartAdmission::new(device.clone(), &[0], 0, &[true]).unwrap();
    {
        let mut state = admission.state.lock();
        state.commit_ipi(HartSet::singleton(0).unwrap()).unwrap();
    }

    assert_eq!(admission.suspend_current(), Ok(()));
    assert_eq!(admission.status(0), Ok(HartState::Started));
    assert_eq!(*device.claimed.lock().unwrap(), [0]);
}

#[test]
fn remote_fence_returns_only_after_target_completion() {
    let device = Arc::new(RecordedIpi::default());
    let admission = two_hart_admission(device.clone());
    let source = admission.clone();
    let request = std::thread::spawn(move || {
        source.remote_fence(
            HartTargets::selected(1, 8),
            RemoteFenceRequest::SfenceVma {
                start: 0x1000,
                size: 0x2000,
            },
        )
    });

    while device.notification_count.load(Ordering::Acquire) == 0 {
        std::thread::yield_now();
    }
    assert!(!request.is_finished());
    admission.drain(8, true).unwrap();
    assert_eq!(request.join().unwrap(), Ok(()));

    let state = admission.state.lock();
    assert!(state.fence_source_idle(0));
    assert!(!state.fence_pending(1, 0));
}

#[test]
fn ticket_lock_serializes_contenders_without_lost_updates() {
    const THREADS: usize = 8;
    const STEPS: usize = 1_000;
    let lock = Arc::new(TicketLock::new(0usize));
    let mut threads = alloc::vec::Vec::new();
    for _ in 0..THREADS {
        let lock = lock.clone();
        threads.push(std::thread::spawn(move || {
            for _ in 0..STEPS {
                *lock.lock() += 1;
            }
        }));
    }
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(*lock.lock(), THREADS * STEPS);
}

#[test]
fn admission_start_waits_for_preparation_and_target_publishes_started() {
    let device = Arc::new(RecordedIpi::default());
    let admission = HartAdmission::new(device.clone(), &[0, 8], 0, &[true, true]).unwrap();
    let source = admission.clone();
    let start = std::thread::spawn(move || source.start(8, NextStage::for_test(0x8020_0000)));

    while device.notification_count.load(Ordering::Acquire) == 0 {
        std::thread::yield_now();
    }
    assert_eq!(admission.status(8), Ok(HartState::StartPending));
    admission.publish_start_result(8, Ok(())).unwrap();
    assert_eq!(start.join().unwrap(), Ok(()));
    assert_eq!(admission.status(8), Ok(HartState::StartPending));

    let _next_stage = admission.take_start(8).unwrap();
    assert_eq!(admission.status(8), Ok(HartState::Started));
}

#[test]
fn admission_start_failure_restores_stopped_and_preserves_error() {
    let device = Arc::new(RecordedIpi::default());
    let admission = HartAdmission::new(device.clone(), &[0, 8], 0, &[true, true]).unwrap();
    let source = admission.clone();
    let start = std::thread::spawn(move || source.start(8, NextStage::for_test(0x8020_0000)));

    while device.notification_count.load(Ordering::Acquire) == 0 {
        std::thread::yield_now();
    }
    admission
        .publish_start_result(8, Err(HartError::NotSupported))
        .unwrap();
    assert_eq!(start.join().unwrap(), Err(HartError::NotSupported));
    assert_eq!(admission.status(8), Ok(HartState::Stopped));
    assert!(admission.state.lock().starts[1].is_none());
}

#[cfg(feature = "hypervisor")]
#[test]
fn hypervisor_fence_requires_every_resolved_target_to_support_h() {
    let mut targets = HartSet::empty();
    targets.insert(0).unwrap();
    targets.insert(1).unwrap();
    let request = RemoteFenceRequest::HfenceGvma {
        start: 0x4000,
        size: 0x1000,
    };
    assert!(!targets_support_request(request, targets, |index| index == 0));
    assert!(targets_support_request(request, targets, |_| true));
    assert!(targets_support_request(
        RemoteFenceRequest::FenceI,
        targets,
        |_| false
    ));
}
