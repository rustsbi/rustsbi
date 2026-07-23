//! Target-executed schedule checks over production admission transitions.

use super::*;

#[crate::mtest]
fn simultaneous_and_late_fence_work_keeps_exact_relations() {
    let mut state =
        HartAdmissionState::new_with_count([0, 1, 2], [HartStatus::Started; 3], [true; 3], 3)
            .unwrap();
    let target = HartSet::singleton(2).unwrap();
    state
        .commit_rfence(0, target, RemoteFenceRequest::FenceI)
        .unwrap();
    state
        .commit_rfence(
            1,
            target,
            RemoteFenceRequest::SfenceVma { start: 0, size: 0 },
        )
        .unwrap();

    let mut claimed = ClaimedWork::default();
    state.claim(2, &mut claimed).unwrap();
    state.complete(2, 0, &mut claimed).unwrap();
    state.retire(0).unwrap();

    state
        .commit_rfence(0, target, RemoteFenceRequest::FenceI)
        .unwrap();
    assert!(claimed.sources.contains(1));
    assert!(!claimed.sources.contains(0));
    assert!(state.fence_targets[2].pending_sources.contains(0));

    state.complete(2, 1, &mut claimed).unwrap();
    state.retire(1).unwrap();
    assert!(state.invariants_hold(&[ClaimedWork::default(), ClaimedWork::default(), claimed]));
}

#[crate::mtest]
fn spurious_and_delayed_notification_cannot_duplicate_ipi_work() {
    let mut state =
        HartAdmissionState::new_with_count([0], [HartStatus::Started], [true], 1).unwrap();
    let target = HartSet::singleton(0).unwrap();
    state.commit_ipi(target).unwrap();
    state.commit_ipi(target).unwrap();

    let mut first = ClaimedWork::default();
    state.claim(0, &mut first).unwrap();
    assert!(first.supervisor_ipi);
    first.supervisor_ipi = false;

    let mut delayed = ClaimedWork::default();
    state.claim(0, &mut delayed).unwrap();
    assert!(delayed.is_empty());
    assert!(state.invariants_hold(&[first]));
}
