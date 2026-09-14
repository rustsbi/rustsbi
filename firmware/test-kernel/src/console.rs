//! Observable return-value and buffer contracts of the SBI DBCN extension.
//!
//! Reference: [SBI v3.0, Section 12, Tables 50-52](https://docs.riscv.org/reference/sbi/_attachments/riscv-sbi.pdf#page=51).
//! Memory-error expectations follow the DBCN-specific tables, whose
//! `INVALID_PARAM` entry differs from Section 3.2's `INVALID_ADDRESS` wording.
//!
//! This kernel runs in Bare mode, so its RAM buffer pointers are physical
//! addresses. No particular UART address, firmware layout, or transfer limit
//! is assumed. A non-zero upper address half is not inherently invalid.
//!
//! DBCN provides no portable way to inject denial or I/O failures, supply known
//! receive data, or observe byte delivery. Those checks need a controlled console
//! fixture; this suite checks the results it can observe without one.

use sbi_spec::binary::{
    Physical, RET_ERR_DENIED, RET_ERR_FAILED, RET_ERR_INVALID_PARAM, RET_SUCCESS, SbiRet,
};
use sbi_testing::sbi;

const BUFFER_LEN: usize = 600;
const CANARY: u8 = 0xa5;
const TEST_CASES: usize = 10;

/// Local test failure codes; these are distinct from SBI return error codes.
#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub(crate) enum ErrorCode {
    UnexpectedReturn = 1,
    ByteCountOutOfRange = 2,
    ReadBeforeBuffer = 3,
    ReadPastBuffer = 4,
    ReadPastCount = 5,
    NonzeroWriteByteValue = 6,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TestError {
    pub code: ErrorCode,
    pub operation: &'static str,
    pub capacity: usize,
    pub returned_error: isize,
    pub returned_value: usize,
}

impl TestError {
    fn new(code: ErrorCode, operation: &'static str, capacity: usize, ret: SbiRet) -> Self {
        Self {
            code,
            operation,
            capacity,
            returned_error: ret.error as isize,
            returned_value: ret.value,
        }
    }
}

pub(crate) enum TestOutcome {
    Unavailable,
    Complete { inconclusive: usize },
}

pub(crate) type TestResult = Result<TestOutcome, [Option<TestError>; TEST_CASES]>;

/// Runs every case and returns failures for the caller to report after other suites.
#[expect(
    clippy::result_large_err,
    reason = "fixed-size reports preserve all failures without requiring a heap allocator"
)]
pub(super) fn test() -> TestResult {
    if sbi::probe_extension(sbi::Console).is_unavailable() {
        return Ok(TestOutcome::Unavailable);
    }

    // Evaluate all cases before collecting errors; one failure cannot skip later
    // cases. Each successful bool records whether data-path checks were inconclusive.
    let results = [
        test_write(0),
        test_write(1),
        test_write(BUFFER_LEN),
        test_read(0),
        test_read(1),
        test_read(16),
        test_read(BUFFER_LEN),
        check_invalid_range(
            "write invalid range",
            sbi::console_write(Physical::new(2, usize::MAX, usize::MAX)),
        ),
        check_invalid_range(
            "read invalid range",
            sbi::console_read(Physical::new(2, usize::MAX, usize::MAX)),
        ),
        test_write_byte(),
    ];
    let inconclusive = results
        .iter()
        .filter(|result| matches!(result, Ok(true)))
        .count();
    let errors = results.map(Result::err);
    if errors.iter().any(Option::is_some) {
        Err(errors)
    } else {
        Ok(TestOutcome::Complete { inconclusive })
    }
}

fn test_write(len: usize) -> Result<bool, TestError> {
    let mut input = [b'.'; BUFFER_LEN];
    input[BUFFER_LEN - 1] = b'\n';

    // Use a valid RAM address even for an empty transfer: DBCN does not
    // explicitly require ignoring arbitrary addresses when num_bytes is 0.
    let ret = sbi::console_write(Physical::new(len, input.as_ptr() as usize, 0));
    // Section 12.1 allows partial writes and zero progress. One call is
    // sufficient; eventual progress is not a conformance requirement.
    Ok(check_transfer("write", ret, len)?.is_none())
}

fn test_read(len: usize) -> Result<bool, TestError> {
    let mut output = [CANARY; BUFFER_LEN + 2];
    let start = output[1..].as_mut_ptr() as usize;
    let ret = sbi::console_read(Physical::new(len, start, 0));
    // Validate the returned count before using it as a slice bound.
    let count = check_transfer("read", ret, len)?;

    // Only the requested destination may be written, including on errors.
    if output[0] != CANARY {
        return Err(TestError::new(
            ErrorCode::ReadBeforeBuffer,
            "read",
            len,
            ret,
        ));
    }
    if !output[1 + len..].iter().all(|byte| *byte == CANARY) {
        return Err(TestError::new(ErrorCode::ReadPastBuffer, "read", len, ret));
    }
    if let Some(count) = count {
        // Section 12.2: the count describes bytes written, and no input
        // means no writes. Do not assume the receive queue is empty or
        // infer byte contents without an external source of known input.
        if !output[1 + count..1 + len]
            .iter()
            .all(|byte| *byte == CANARY)
        {
            return Err(TestError::new(ErrorCode::ReadPastCount, "read", len, ret));
        }
    }
    // On an error, Section 3 leaves a1 unspecified and Section 12.2 does
    // not promise rollback of writes within the destination.
    Ok(count.is_none())
}

fn check_invalid_range(operation: &'static str, ret: SbiRet) -> Result<bool, TestError> {
    // This two-byte range overflows the entire 2*XLEN address encoding, not
    // merely its low half. It cannot describe valid contiguous physical memory.
    // No assumption about physical address 0, MMIO, or firmware RAM is needed.
    // Compare only error: a1 is unspecified on these error paths.
    match ret.error {
        RET_ERR_INVALID_PARAM => Ok(false),
        // The tables do not specify error precedence. Denial or failure
        // may prevent observing the memory-validation result.
        RET_ERR_DENIED | RET_ERR_FAILED => Ok(true),
        _ => Err(TestError::new(
            ErrorCode::UnexpectedReturn,
            operation,
            2,
            ret,
        )),
    }
}

fn test_write_byte() -> Result<bool, TestError> {
    let ret = sbi::console_write_byte(b'\n');
    if !matches!(ret.error, RET_SUCCESS | RET_ERR_DENIED | RET_ERR_FAILED) {
        return Err(TestError::new(
            ErrorCode::UnexpectedReturn,
            "write_byte",
            1,
            ret,
        ));
    }
    // Section 12.3 explicitly defines a1 as zero on every return, including
    // errors, overriding Section 3's general rule for error returns.
    if ret.value != 0 {
        return Err(TestError::new(
            ErrorCode::NonzeroWriteByteValue,
            "write_byte",
            1,
            ret,
        ));
    }
    Ok(ret.error != RET_SUCCESS)
}

/// Checks Tables 50-51 for buffers in the test kernel's accessible RAM.
fn check_transfer(
    operation: &'static str,
    ret: SbiRet,
    capacity: usize,
) -> Result<Option<usize>, TestError> {
    match ret.error {
        RET_SUCCESS if ret.value <= capacity => Ok(Some(ret.value)),
        RET_SUCCESS => Err(TestError::new(
            ErrorCode::ByteCountOutOfRange,
            operation,
            capacity,
            ret,
        )),
        RET_ERR_DENIED | RET_ERR_FAILED => Ok(None),
        // INVALID_PARAM is not expected for these valid buffers. Other SBI
        // errors, including NOT_SUPPORTED, are absent from Tables 50-51 when
        // DBCN is advertised and a defined function is called.
        _ => Err(TestError::new(
            ErrorCode::UnexpectedReturn,
            operation,
            capacity,
            ret,
        )),
    }
}
