//! Debug console extension test suite.

use sbi::SbiRet;
use sbi_spec::binary::Physical;

/// Debug console extension test cases.
#[derive(Clone, Debug)]
pub enum Case {
    /// Can't proceed test for debug console extension does not exist.
    NotExist,
    /// Test begin.
    Begin,
    /// Test process for write a byte to console.
    WriteByte,
    /// Test failed for can't write byte.
    WritingByteFailed(SbiRet),
    /// Test process for write complete slice.
    WriteSlice,
    /// Test process for write partial slice.
    WritingPartialSlice(usize),
    /// Test failed for can't write slice.
    WritingSliceFailed(SbiRet),
    /// Test process for read some bytes from console.
    Read(usize),
    /// Test failed for can't read to buffer.
    ReadingFailed(SbiRet),
    /// Test process for rejecting a write with a non-zero upper address half.
    NonzeroUpperWriteRejected(SbiRet),
    /// Test failed because a write with a non-zero upper address half was accepted.
    NonzeroUpperWriteAccepted(usize),
    /// Test process for rejecting a read with a non-zero upper address half.
    NonzeroUpperReadRejected(SbiRet),
    /// Test failed because a read with a non-zero upper address half was accepted.
    NonzeroUpperReadAccepted(usize),
    /// All test cases on debug console extension has passed.
    Pass,
}

/// Test debug console extension.
pub fn test(mut f: impl FnMut(Case)) {
    if sbi::probe_extension(sbi::Console).is_unavailable() {
        f(Case::NotExist);
        return;
    }

    f(Case::Begin);

    let ret = sbi::console_write_byte(b'H');
    if ret.is_ok() {
        f(Case::WriteByte);
    } else {
        f(Case::WritingByteFailed(ret));
    }
    let words = b"ello, world!\r\n";
    let ret = sbi::console_write(Physical::new(words.len(), words.as_ptr() as _, 0));
    if let Some(len) = ret.ok() {
        f(if len == words.len() {
            Case::WriteSlice
        } else {
            Case::WritingPartialSlice(len)
        });
    } else {
        f(Case::WritingSliceFailed(ret));
    }
    let mut buffer = [0u8; 16];
    let ret = sbi::console_read(Physical::new(buffer.len(), buffer.as_mut_ptr() as _, 0));
    if let Some(len) = ret.ok() {
        f(Case::Read(len));
    } else {
        f(Case::ReadingFailed(ret));
    }

    #[cfg(target_pointer_width = "64")]
    {
        let nonzero_upper = 1usize << 32;

        let ret = sbi::console_write(Physical::new(
            words.len(),
            words.as_ptr() as _,
            nonzero_upper,
        ));
        if let Some(len) = ret.ok() {
            f(Case::NonzeroUpperWriteAccepted(len));
        } else {
            f(Case::NonzeroUpperWriteRejected(ret));
        }

        let ret = sbi::console_read(Physical::new(
            buffer.len(),
            buffer.as_mut_ptr() as _,
            nonzero_upper,
        ));
        if let Some(len) = ret.ok() {
            f(Case::NonzeroUpperReadAccepted(len));
        } else {
            f(Case::NonzeroUpperReadRejected(ret));
        }
    }

    f(Case::Pass);
}
