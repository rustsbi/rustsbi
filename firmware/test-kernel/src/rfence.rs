//! SBI remote-fence and hypervisor-fence smoke tests.
//!
//! These checks execute real SBI ecalls and inspect their return values.
//! They do not install page tables or verify translation invalidation effects.

use sbi_spec::binary::{HartMask, SbiRet};
use sbi_testing::sbi;

/// Exercises the RFENCE calls for the current hart and checks an invalid
/// hart ID for the calls whose target validation is observable.
pub(crate) fn test(hartid: usize, smp: usize, dtb_pa: usize) {
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

    test_local_ranges(self_mask);
    test_hypervisor_ranges(self_mask, hart_has_h(hartid, dtb_pa));

    println!("[rfence] local return checks pass; translation invalidation effects not checked");
}

const PAGE_SIZE: usize = 4096;
type RangeFence = fn(HartMask, usize, usize) -> SbiRet;

fn test_local_ranges(mask: HartMask) {
    assert_eq!(sbi::remote_fence_i(mask), SbiRet::success(0));
    let calls: [(&str, RangeFence); 2] = [
        ("SFENCE.VMA", sbi::remote_sfence_vma),
        ("SFENCE.VMA with ASID", |mask, start, size| {
            sbi::remote_sfence_vma_asid(mask, start, size, 0)
        }),
    ];
    let last_page = usize::MAX & !(PAGE_SIZE - 1);
    for (name, call) in calls {
        for (start, size) in [(0, 0), (PAGE_SIZE, PAGE_SIZE + 1), (PAGE_SIZE, usize::MAX)] {
            assert_eq!(
                call(mask, start, size),
                SbiRet::success(0),
                "{name}: start={start:#x}, size={size:#x}"
            );
        }
        for (start, size) in [(1, PAGE_SIZE), (last_page, PAGE_SIZE)] {
            assert_eq!(
                call(mask, start, size),
                SbiRet::invalid_address(),
                "{name}: invalid start={start:#x}, size={size:#x}"
            );
        }
    }
    println!(
        "[rfence] FENCE.I and SFENCE.VMA: all, partial, nonzero-start full range and invalid ranges pass"
    );
}

fn test_hypervisor_ranges(mask: HartMask, platform_h: Option<bool>) {
    let calls: [(&str, RangeFence); 4] = [
        ("HFENCE.GVMA", sbi::remote_hfence_gvma),
        ("HFENCE.GVMA with VMID", |mask, start, size| {
            sbi::remote_hfence_gvma_vmid(mask, start, size, 0)
        }),
        ("HFENCE.VVMA", sbi::remote_hfence_vvma),
        ("HFENCE.VVMA with ASID", |mask, start, size| {
            sbi::remote_hfence_vvma_asid(mask, start, size, 0)
        }),
    ];
    for (name, call) in calls {
        let result = call(mask, 0, 0);
        assert!(
            result == SbiRet::success(0) || result == SbiRet::not_supported(),
            "{name}: {result:?}"
        );
        // A compiled-out operation and an unsupported executing hart must
        // return NOT_SUPPORTED for both partial and full-range requests.
        for (start, size) in [(PAGE_SIZE, PAGE_SIZE + 1), (PAGE_SIZE, usize::MAX)] {
            assert_eq!(
                call(mask, start, size),
                result,
                "{name}: start={start:#x}, size={size:#x}"
            );
        }
        if result.is_ok() {
            if platform_h == Some(false) {
                println!("[rfence] {name}: hardware supports the operation despite DT omitting H");
            }
            println!(
                "[rfence] {name}: execution path returned SUCCESS for all, partial and full ranges"
            );
        } else {
            let reason = match platform_h {
                Some(false) => "platform advertises no H extension",
                Some(true) => "platform advertises H; firmware operation unavailable",
                None => "platform H capability unknown",
            };
            println!("[rfence] {name}: NOT_SUPPORTED ({reason}); instruction execution skipped");
        }
    }
}

/// Reads the selected CPU's advertised H extension without accessing M-mode
/// CSRs or executing optional H instructions in the test kernel.
fn hart_has_h(hartid: usize, dtb_pa: usize) -> Option<bool> {
    use dtb_walker::{Dtb, DtbObj, HeaderError as E, Property, Str, WalkOperation::*};

    let mut extensions_h = None;
    let mut legacy_h = None;
    // SAFETY: the boot-provided DTB remains readable and unmodified during
    // these tests. `platform::initialize` has already read the same header.
    unsafe {
        Dtb::from_raw_parts_filtered(dtb_pa as _, |error| {
            matches!(error, E::Misaligned(4) | E::LastCompVersion(_))
        })
    }
    .unwrap()
    .walk(|ctx, obj| match obj {
        DtbObj::SubNode { name } => {
            if (ctx.is_root() && name == Str::from("cpus"))
                || (ctx.name() == Str::from("cpus")
                    && name
                        .as_str()
                        .ok()
                        .and_then(|name| name.strip_prefix("cpu@"))
                        .and_then(|id| usize::from_str_radix(id, 16).ok())
                        == Some(hartid))
            {
                StepInto
            } else {
                StepOver
            }
        }
        DtbObj::Property(Property::General { name, value }) => {
            if name == Str::from("riscv,isa-extensions") {
                extensions_h = Some(value.split(|byte| *byte == 0).any(|name| name == b"h"));
            } else if name == Str::from("riscv,isa") {
                let base = value.split(|byte| *byte == b'_').next().unwrap_or_default();
                legacy_h = Some(base.contains(&b'h'));
            }
            StepOver
        }
        DtbObj::Property(_) => StepOver,
    });
    extensions_h.or(legacy_h)
}
