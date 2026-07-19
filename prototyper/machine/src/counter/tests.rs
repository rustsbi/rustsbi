use super::state::CounterFacts;
use super::*;

fn facts(mask: u32) -> CounterFacts {
    CounterFacts {
        accessible: mask,
        controllable: mask,
        wide_events: mask & !0b111,
        initialized: true,
    }
}

#[test]
fn access_permission_does_not_publish_uncontrollable_pmu_counters() {
    let facts = CounterFacts {
        accessible: (1 << 0) | (1 << 2) | (1 << 7),
        controllable: 1 << 7,
        wide_events: 1 << 7,
        initialized: true,
    };
    assert_eq!(facts.accessible.count_ones(), 3);
    assert_eq!(facts.count(), 1);
    assert_eq!(facts.counter(0).unwrap().csr_number, 0xc07);
    assert_eq!(facts.counter(1), None);
}

#[test]
fn dense_indices_skip_absent_and_time_counters() {
    let counters = facts((1 << 0) | (1 << 2) | (1 << 7));
    assert_eq!(counters.count(), 3);
    assert_eq!(counters.counter(0).unwrap().csr_number, 0xc00);
    assert_eq!(counters.counter(1).unwrap().csr_number, 0xc02);
    assert_eq!(counters.counter(2).unwrap().csr_number, 0xc07);
    assert_eq!(counters.counter(3), None);
}

#[test]
fn identifiers_are_revalidated_against_current_facts() {
    let source = facts((1 << 0) | (1 << 7));
    let target = facts(1 << 0);
    let counter = source.counter(1).unwrap();
    assert_eq!(target.validate(counter), Err(CounterError::InvalidCounter));

    let forged_internal = CounterId {
        offset: 0,
        csr_number: 0xc07,
    };
    assert_eq!(
        source.validate(forged_internal),
        Err(CounterError::InvalidCounter)
    );
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
