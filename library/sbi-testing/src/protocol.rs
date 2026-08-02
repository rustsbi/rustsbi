//! Allocation-free structured result records for serial test transports.

use core::fmt::{self, Write};

/// Current structured-result protocol version.
pub const VERSION: u32 = 1;

/// Immutable identity and reproducibility fields carried by one test run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Run<'a> {
    /// Stable matrix shard identifier.
    pub shard: &'a str,
    /// Stable invocation identifier within the shard.
    pub run: &'a str,
    /// One-based execution attempt. Acceptance runs always use one.
    pub attempt: u32,
    /// Deterministic schedule or input seed.
    pub seed: u64,
    /// SHA-256 digest of the ordered expected case identifiers.
    pub digest: &'a str,
}

/// Writes the authoritative beginning of a run.
pub fn run_start(output: &mut impl Write, run: Run<'_>, expected: usize) -> fmt::Result {
    writeln!(
        output,
        "@@RUSTSBI_TEST v={VERSION} type=RUN_START shard={} run={} attempt={} seed={:016x} digest={} expected={expected}",
        run.shard, run.run, run.attempt, run.seed, run.digest
    )
}

/// Writes the authoritative beginning of one planned case.
pub fn case_start(output: &mut impl Write, run: Run<'_>, case: &str) -> fmt::Result {
    writeln!(
        output,
        "@@RUSTSBI_TEST v={VERSION} type=CASE_START shard={} run={} case={case}",
        run.shard, run.run
    )
}

/// Writes the successful terminal record for one planned case.
pub fn case_pass(output: &mut impl Write, run: Run<'_>, case: &str) -> fmt::Result {
    writeln!(
        output,
        "@@RUSTSBI_TEST v={VERSION} type=CASE_PASS shard={} run={} case={case} diag=OK",
        run.shard, run.run
    )
}

/// Writes the failed terminal record for one planned case.
pub fn case_fail(
    output: &mut impl Write,
    run: Run<'_>,
    case: &str,
    diagnostic: &str,
) -> fmt::Result {
    writeln!(
        output,
        "@@RUSTSBI_TEST v={VERSION} type=CASE_FAIL shard={} run={} case={case} diag={diagnostic}",
        run.shard, run.run
    )
}

/// Writes a terminal harness failure that did not return through a test ABI.
pub fn harness_fail(output: &mut impl Write, run: Run<'_>, diagnostic: &str) -> fmt::Result {
    writeln!(
        output,
        "@@RUSTSBI_TEST v={VERSION} type=HARNESS_FAIL shard={} run={} diag={diagnostic}",
        run.shard, run.run
    )
}

/// Writes the authoritative successful end of a run.
pub fn run_pass(output: &mut impl Write, run: Run<'_>, passed: usize) -> fmt::Result {
    writeln!(
        output,
        "@@RUSTSBI_TEST v={VERSION} type=RUN_END shard={} run={} outcome=PASS passed={passed} failed=0 digest={} diag=OK",
        run.shard, run.run, run.digest
    )
}

/// Writes the authoritative failed end of a run.
pub fn run_fail(
    output: &mut impl Write,
    run: Run<'_>,
    passed: usize,
    failed: usize,
    diagnostic: &str,
) -> fmt::Result {
    writeln!(
        output,
        "@@RUSTSBI_TEST v={VERSION} type=RUN_END shard={} run={} outcome=FAIL passed={passed} failed={failed} digest={} diag={diagnostic}",
        run.shard, run.run, run.digest
    )
}
