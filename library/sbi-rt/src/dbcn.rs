//! Chapter 12. Debug Console Extension (EID #0x4442434E "DBCN")
use crate::binary::{sbi_call_1, sbi_call_3};
use sbi_spec::{
    binary::{Physical, SbiRet},
    dbcn::{CONSOLE_READ, CONSOLE_WRITE, CONSOLE_WRITE_BYTE, EID_DBCN},
};

/// Write bytes to the debug console from input memory.
///
/// # Parameters
///
/// The `bytes` parameter specifies the input memory, including its length
/// and memory physical base address (both lower and upper bits).
///
/// # Non-blocking function
///
/// This is a non-blocking SBI call, and it may do partial or no write operations if
/// the debug console is not able to accept more bytes.
///
/// # Return value
///
/// The number of bytes written is returned in `SbiRet.value` and the
/// possible return error codes returned in `SbiRet.error` are shown in
/// the table below:
///
/// | Return code               | Description
/// |:--------------------------|:----------------------------------------------
/// | `SbiRet::success()`       | Bytes written successfully.
/// | `SbiRet::invalid_param()` | The memory pointed to by `bytes` does not satisfy the requirements described in shared memory physical address range.
/// | `SbiRet::failed()`        | Failed to write due to I/O errors.
///
/// This function is defined in RISC-V SBI Specification chapter 12.1.
#[inline]
pub fn console_write(bytes: Physical<&[u8]>) -> SbiRet {
    sbi_call_3(
        EID_DBCN,
        CONSOLE_WRITE,
        bytes.num_bytes(),
        bytes.phys_addr_lo(),
        bytes.phys_addr_hi(),
    )
}

/// Read bytes from the debug console into an output memory.
///
/// # Parameters
///
/// The `bytes` parameter specifies the output memory, including the maximum
/// bytes which can be written, and its memory physical base address
/// (both lower and upper bits).
///
/// # Non-blocking function
///
/// This is a non-blocking SBI call, and it will not write anything
/// into the output memory if there are no bytes to be read in the
/// debug console.
///
/// # Return value
///
/// The number of bytes read is returned in `SbiRet.value` and the
/// possible return error codes returned in `SbiRet.error` are shown in
/// the table below:
///
/// | Return code               | Description
/// |:--------------------------|:----------------------------------------------
/// | `SbiRet::success()`       | Bytes read successfully.
/// | `SbiRet::invalid_param()` | The memory pointed to by `bytes` does not satisfy the requirements described in shared memory physical address range.
/// | `SbiRet::failed()`        | Failed to read due to I/O errors.
///
/// This function is defined in RISC-V SBI Specification chapter 12.2.
pub fn console_read(bytes: Physical<&mut [u8]>) -> SbiRet {
    sbi_call_3(
        EID_DBCN,
        CONSOLE_READ,
        bytes.num_bytes(),
        bytes.phys_addr_lo(),
        bytes.phys_addr_hi(),
    )
}

/// Write a single byte to the debug console.
///
/// # Blocking function
///
/// This is a blocking SBI call, and it will only return after writing
/// the specified byte to the debug console. It will also return with
/// `SbiRet::failed()` if there are I/O errors.
/// # Return value
///
/// The `SbiRet.value` is set to zero, and the possible return error
/// codes returned in `SbiRet.error` are shown in the table below:
///
/// | Return code               | Description
/// |:--------------------------|:----------------------------------------------
/// | `SbiRet::success()`       | Byte written successfully.
/// | `SbiRet::failed()`        | Failed to write the byte due to I/O errors.
///
/// This function is defined in RISC-V SBI Specification chapter 12.3.
#[inline]
pub fn console_write_byte(byte: u8) -> SbiRet {
    sbi_call_1(EID_DBCN, CONSOLE_WRITE_BYTE, byte as usize)
}
