//! Firmware-feature operations through the actual SBI ecall path.
//!
//! Unsupported hardware fields are reported individually. Positive cases
//! verify readback, preservation of other features, and restoration. This
//! suite does not inject guarded-access or restoration faults.

use sbi_spec::{binary::SbiRet, fwft::feature_type};
use sbi_testing::sbi;

const FEATURES: [(usize, &str); 6] = [
    (feature_type::MISALIGNED_EXC_DELEG, "misaligned delegation"),
    (feature_type::LANDING_PAD, "landing pad"),
    (feature_type::SHADOW_STACK, "shadow stack"),
    (feature_type::DOUBLE_TRAP, "double trap"),
    (feature_type::PTE_AD_HW_UPDATING, "PTE A/D updating"),
    (feature_type::POINTER_MASKING_PMLEN, "pointer masking"),
];

pub(crate) fn test() {
    if sbi::probe_extension(sbi::Fwft).is_unavailable() {
        println!("[fwft] skipped: extension unavailable");
        return;
    }

    // Reserved and platform identifiers may have different error codes.
    assert!(sbi::fwft_get(u32::MAX).is_err());
    assert!(sbi::fwft_set(u32::MAX, 0, 0).is_err());
    for value in [1, 2, 3, 8, 17, usize::MAX] {
        assert_eq!(
            sbi::fwft_set(feature_type::POINTER_MASKING_PMLEN as u32, value, 0),
            SbiRet::invalid_param(),
            "invalid pointer-masking length {value}"
        );
    }

    let original = FEATURES.map(|(id, name)| {
        let result = sbi::fwft_get(id as u32);
        if result == SbiRet::not_supported() {
            if id == feature_type::POINTER_MASKING_PMLEN {
                for length in [7, 16] {
                    assert_eq!(
                        sbi::fwft_set(id as u32, length, 0),
                        SbiRet::not_supported(),
                        "FWFT get missed supported pointer masking PMLEN={length}"
                    );
                }
            }
            println!("[fwft] {name}: skipped, hardware field unavailable");
            None
        } else {
            assert!(result.is_ok(), "FWFT get {name}: {result:?}");
            Some(result.value)
        }
    });
    let mut supported = 0;
    for (index, &(id, name)) in FEATURES.iter().enumerate() {
        let Some(current) = original[index] else {
            continue;
        };
        supported += 1;
        assert_eq!(sbi::fwft_set(id as u32, current, 0), SbiRet::success(0));
        assert_eq!(
            sbi::fwft_set(id as u32, current, 2),
            SbiRet::invalid_param()
        );
        if id == feature_type::POINTER_MASKING_PMLEN {
            assert!(matches!(current, 0 | 7 | 16));
            for length in [0, 7, 16] {
                let result = sbi::fwft_set(id as u32, length, 0);
                if result == SbiRet::not_supported() {
                    assert_eq!(sbi::fwft_get(id as u32), SbiRet::success(current));
                    check_other_features(&original, index);
                    println!("[fwft] pointer masking PMLEN={length}: unavailable, value preserved");
                } else {
                    assert_eq!(result, SbiRet::success(0));
                    assert_eq!(sbi::fwft_get(id as u32), SbiRet::success(length));
                    // Repeated reads must preserve the field's visible value.
                    assert_eq!(sbi::fwft_get(id as u32), SbiRet::success(length));
                    check_other_features(&original, index);
                    assert_eq!(sbi::fwft_set(id as u32, current, 0), SbiRet::success(0));
                    assert_eq!(sbi::fwft_get(id as u32), SbiRet::success(current));
                    println!("[fwft] pointer masking PMLEN={length}: readback and restore pass");
                }
            }
        } else {
            assert!(current <= 1);
            let next = 1 - current;
            assert_eq!(sbi::fwft_set(id as u32, next, 0), SbiRet::success(0));
            assert_eq!(sbi::fwft_get(id as u32), SbiRet::success(next));
            check_other_features(&original, index);
            assert_eq!(sbi::fwft_set(id as u32, 2, 0), SbiRet::invalid_param());
            assert_eq!(sbi::fwft_get(id as u32), SbiRet::success(next));
            assert_eq!(sbi::fwft_set(id as u32, current, 0), SbiRet::success(0));
            assert_eq!(sbi::fwft_get(id as u32), SbiRet::success(current));
            println!("[fwft] {name}: readback and restore pass");
        }
    }
    println!(
        "[fwft] return checks pass; {supported}/{} hardware fields exercised",
        FEATURES.len()
    );
}

fn check_other_features(original: &[Option<usize>; FEATURES.len()], changed: usize) {
    for (index, &(id, name)) in FEATURES.iter().enumerate() {
        if index != changed
            && let Some(value) = original[index]
        {
            assert_eq!(
                sbi::fwft_get(id as u32),
                SbiRet::success(value),
                "FWFT changed unrelated feature {name}"
            );
        }
    }
}
