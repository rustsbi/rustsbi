//! Dedicated one-case M-mode test runner and protocol reporting.

use core::fmt::{self, Write};

use sbi_testing::protocol::{self, Run};

use super::{fail, logger, platform};

const DEFAULT_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

struct Output(machine::Console);

impl Write for Output {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.0
            .write_fmt(format_args!("{value}"))
            .map_err(|_| fmt::Error)
    }
}

fn run_metadata() -> Run<'static> {
    Run {
        shard: option_env!("RUSTSBI_TEST_SHARD").unwrap_or("mtest"),
        run: option_env!("RUSTSBI_TEST_RUN_ID").unwrap_or("mtest"),
        attempt: 1,
        seed: 0,
        digest: option_env!("RUSTSBI_TEST_DIGEST").unwrap_or(DEFAULT_DIGEST),
    }
}

pub(super) fn run(mut boot: machine::BootInfo) -> ! {
    let mut tree = platform::parse(&*boot.dtb_mut()).unwrap_or_else(|_| fail());
    let supervisor_ram = platform::memory(&tree).unwrap_or_else(|_| fail());
    let discovered_harts = platform::discover_harts(&tree).unwrap_or_else(|_| fail());
    let hart_count = discovered_harts.len();

    let _ = platform::install_timer_and_ipi(&mut boot, &mut tree, &discovered_harts)
        .unwrap_or_else(|_| fail());
    let console = platform::install_console(&mut tree).unwrap_or_else(|_| fail());
    let console = console.unwrap_or_else(fail);
    logger::install(console.clone(), hart_count).unwrap_or_else(|_| fail());
    let power = platform::install_power(&mut tree).unwrap_or_else(|_| fail());
    if !power {
        return fail();
    }
    let protection = machine::pmp::config! {
        supervisor_ram => [read, write, execute];
    }
    .unwrap_or_else(|_| fail());
    boot.set_memory_protection(protection)
        .unwrap_or_else(|_| fail());
    let _ = boot.supervisor_memory().unwrap_or_else(|_| fail());
    let _ = boot.performance_counters().unwrap_or_else(|_| fail());
    platform::finish_device_tree(tree, boot.dtb_mut()).unwrap_or_else(|_| fail());

    let tests = machine::prepare_tests(boot, console.clone());
    let metadata = run_metadata();
    let mut output = Output(console);
    if option_env!("RUSTSBI_MTEST_LIST").is_some() {
        let mut failed = false;
        tests.visit(|name| {
            failed |= writeln!(output, "@@RUSTSBI_MTEST type=CASE name={name}").is_err();
        });
        terminate(if failed {
            machine::power::PowerReason::SystemFailure
        } else {
            machine::power::PowerReason::Unspecified
        })
    }

    let filter = option_env!("RUSTSBI_MTEST_FILTER").unwrap_or("");
    let Some(test) = tests.select(filter) else {
        let _ = protocol::harness_fail(&mut output, metadata, "TEST_NOT_FOUND");
        terminate(machine::power::PowerReason::SystemFailure)
    };

    if protocol::run_start(&mut output, metadata, 1).is_err()
        || protocol::case_start(&mut output, metadata, test.name()).is_err()
    {
        terminate(machine::power::PowerReason::SystemFailure)
    }
    let name = test.name();
    test.run();
    if protocol::case_pass(&mut output, metadata, name).is_err()
        || protocol::run_pass(&mut output, metadata, 1).is_err()
    {
        terminate(machine::power::PowerReason::SystemFailure)
    }
    terminate(machine::power::PowerReason::Unspecified)
}

pub(super) fn report_panic() {
    let metadata = run_metadata();
    logger::try_report_test_failure(metadata.shard, metadata.run, "UNEXPECTED_PANIC")
}

fn terminate(reason: machine::power::PowerReason) -> ! {
    let _ = machine::power::shutdown(reason);
    fail()
}
