//! Dedicated one-case M-mode test runner and protocol reporting.

use core::fmt::{self, Write};

use sbi_testing::protocol::{self, Run};

use super::{Initialized, fail, initialize, logger};

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

pub(super) fn run(boot: machine::BootInfo) -> ! {
    let Initialized {
        boot,
        power,
        console,
        timer: _,
        ipi: _,
        harts: _,
        fence: _,
        memory: _,
        counters: _,
        hart_count: _,
    } = initialize(boot);
    let console = console.unwrap_or_else(fail);
    let power = power.unwrap_or_else(fail);
    let tests = machine::prepare_tests(boot, console.clone());
    let metadata = run_metadata();
    let mut output = Output(console);
    if option_env!("RUSTSBI_MTEST_LIST").is_some() {
        let mut failed = false;
        tests.visit(|name| {
            failed |= writeln!(output, "@@RUSTSBI_MTEST type=CASE name={name}").is_err();
        });
        terminate(
            power,
            if failed {
                machine::PowerReason::SystemFailure
            } else {
                machine::PowerReason::Unspecified
            },
        )
    }

    let filter = option_env!("RUSTSBI_MTEST_FILTER").unwrap_or("");
    let Some(test) = tests.select(filter) else {
        let _ = protocol::harness_fail(&mut output, metadata, "TEST_NOT_FOUND");
        terminate(power, machine::PowerReason::SystemFailure)
    };

    if protocol::run_start(&mut output, metadata, 1).is_err()
        || protocol::case_start(&mut output, metadata, test.name()).is_err()
    {
        terminate(power, machine::PowerReason::SystemFailure)
    }
    let name = test.name();
    test.run();
    if protocol::case_pass(&mut output, metadata, name).is_err()
        || protocol::run_pass(&mut output, metadata, 1).is_err()
    {
        terminate(power, machine::PowerReason::SystemFailure)
    }
    terminate(power, machine::PowerReason::Unspecified)
}

pub(super) fn report_panic() {
    let metadata = run_metadata();
    logger::try_report_test_failure(metadata.shard, metadata.run, "UNEXPECTED_PANIC")
}

fn terminate(power: machine::Power, reason: machine::PowerReason) -> ! {
    power.shutdown(reason);
    fail()
}
