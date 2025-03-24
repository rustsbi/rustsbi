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

    f(Case::Pass);
}
