use super::hart::HartCounters;
use super::*;

fn facts(mask: u32) -> HartCounters {
    HartCounters {
        accessible: mask,
        controllable: mask,
        wide_events: mask & !0b111,
        initialized: true,
    }
}

#[test]
fn access_permission_does_not_publish_uncontrollable_pmu_counters() {
    let facts = HartCounters {
        accessible: (1 << 0) | (1 << 2) | (1 << 7),
        controllable: 1 << 7,
        wide_events: 1 << 7,
        initialized: true,
    };
    assert_eq!(facts.accessible.count_ones(), 3);
    assert_eq!(facts.count(), 1);
    assert_eq!(facts.offset(0), Some(7));
    assert_eq!(facts.offset(1), None);
}

#[test]
fn dense_indices_skip_absent_and_time_counters() {
    let counters = facts((1 << 0) | (1 << 2) | (1 << 7));
    assert_eq!(counters.count(), 3);
    assert_eq!(counters.offset(0), Some(0));
    assert_eq!(counters.offset(1), Some(2));
    assert_eq!(counters.offset(2), Some(7));
    assert_eq!(counters.offset(3), None);
}

#[test]
fn dense_indices_are_revalidated_against_current_hart_facts() {
    let source = facts((1 << 0) | (1 << 7));
    let target = facts(1 << 0);
    assert_eq!(source.validate(1), Ok(7));
    assert_eq!(target.validate(1), Err(CounterError::InvalidCounter));
}

#[test]
fn counter_info_reports_architectural_width_not_sbi_packed_width() {
    let info = CounterInfo {
        csr_number: 0xc03,
        width: 64,
    };
    assert_eq!(info.csr_number(), 0xc03);
    assert_eq!(info.width(), 64);
}
