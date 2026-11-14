//! HSM entry-address validation and repeated start/stop wake regressions.

use fdt::Fdt;
use sbi_spec::binary::SbiRet;
use sbi_spec::hsm::hart_state;
use sbi_testing::HsmCase;
use sbi_testing::sbi;

/// Rounds of the generic HSM lifecycle test used as wake regression evidence.
const STRESS_ROUNDS: usize = 16;

/// Runs the HSM entry-address checks, then the lifecycle stress rounds.
///
/// The firmware hands over consecutive hart IDs `0..smp`, and every secondary
/// is `STOPPED` once the generic `sbi-testing` HSM suite has finished, so the
/// system-suspend entry criteria hold before this suite runs.
pub(crate) fn test(primary_hart: usize, smp: usize, dtb_pa: usize) {
    let firmware_reservation = firmware_reservation_address(dtb_pa);
    // `usize::MAX` belongs to no representable RAM range; the firmware
    // reservation lies in RAM but the PMP layout forbids S-mode execution
    // there.
    for address in [usize::MAX, firmware_reservation] {
        if smp > 1 {
            let target = (primary_hart + 1) % smp;
            assert_eq!(
                sbi::hart_start(target, address, 0),
                SbiRet::invalid_address(),
                "hart_start must reject {address:#x}"
            );
            assert_eq!(
                sbi::hart_get_status(target),
                SbiRet::success(hart_state::STOPPED),
                "rejected start must leave hart {target} stopped"
            );
        }
        assert_eq!(
            sbi::hart_suspend(sbi::NonRetentive, address, 0),
            SbiRet::invalid_address(),
            "non-retentive hart_suspend must reject {address:#x}"
        );
        assert_eq!(
            sbi::hart_get_status(primary_hart),
            SbiRet::success(hart_state::STARTED),
            "rejected suspend must leave the current hart started"
        );
        assert_eq!(
            sbi::system_suspend(sbi::SuspendToRam, address, 0),
            SbiRet::invalid_address(),
            "system_suspend must reject {address:#x}"
        );
        assert_eq!(
            sbi::hart_get_status(primary_hart),
            SbiRet::success(hart_state::STARTED),
            "rejected system suspend must leave the current hart started"
        );
    }
    println!("[hsm] invalid entry addresses pass");

    if smp > 1 {
        stress_lifecycle(primary_hart, smp);
        println!("[hsm] repeated start/stop wakeups pass");
    }
}

/// Repeats the generic HSM start/suspend/stop lifecycle for `STRESS_ROUNDS`.
///
/// Each round starts every stopped secondary, suspends it non-retentively,
/// resumes it, suspends it retentively, and stops it again, so any lost
/// wakeup between the published stage and the wake backend stalls the boot
/// until the QEMU timeout instead of passing.
fn stress_lifecycle(primary_hart: usize, smp: usize) {
    for round in 0..STRESS_ROUNDS {
        let mut passed = false;
        sbi_testing::test_hsm(primary_hart, (1usize << smp) - 1, 0, |case| match case {
            HsmCase::Pass => passed = true,
            HsmCase::NotExist => panic!("[hsm] round {round}: HSM extension disappeared"),
            HsmCase::NoStoppedHart => panic!("[hsm] round {round}: no stopped hart to restart"),
            HsmCase::HartStartedBeforeTest(hart) => {
                panic!("[hsm] round {round}: hart {hart} started before the lifecycle test")
            }
            HsmCase::HartStartFailed { hartid, ret } => {
                panic!("[hsm] round {round}: hart_start failed on hart {hartid}: {ret:?}")
            }
            HsmCase::RemoteRFenceFailed(hart, ret) => {
                panic!("[hsm] round {round}: remote fence failed on hart {hart}: {ret:?}")
            }
            _ => {}
        });
        assert!(passed, "[hsm] round {round} lifecycle test did not pass");
    }
}

/// Reads the firmware reservation's start address from the next-stage DTB.
///
/// Firmware handoff patches the tree with an `mmode_resv1@` child under
/// `/reserved-memory` covering the firmware image, whose PMP entries forbid
/// S-mode execution. A missing node or `reg` entry is a hard failure so the
/// PMP rejection case cannot silently degrade to the RAM-only case.
fn firmware_reservation_address(dtb_pa: usize) -> usize {
    // SAFETY: the SBI next-stage ABI supplies a readable FDT at `dtb_pa`, and
    // the tree stays unchanged while the test kernel runs.
    let fdt = unsafe { Fdt::from_ptr(dtb_pa as *const u8) }
        .expect("the next-stage DTB must be parseable");
    let reserved_memory = fdt
        .find_node("/reserved-memory")
        .expect("the next-stage DTB must describe /reserved-memory");
    for child in reserved_memory.children() {
        if !child.name.starts_with("mmode_resv1@") {
            continue;
        }
        let region = child
            .reg()
            .and_then(|mut reg| reg.next())
            .expect("the mmode_resv1 node must carry a reg entry");
        let address = region.starting_address as usize;
        assert_ne!(address, usize::MAX, "the firmware reservation is empty");
        return address;
    }
    panic!("the next-stage DTB lacks a firmware mmode_resv1 reservation");
}
