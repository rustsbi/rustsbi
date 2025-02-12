//! Chapter 16. Steal-time Accounting Extension (EID #0x535441 "STA")

use crate::binary::sbi_call_3;

use sbi_spec::{
    binary::{SbiRet, SharedPtr},
    sta::{EID_STA, SET_SHMEM},
};

/// Prepare shared memory for steal-time accounting feature.
///
/// Set the shared memory physical base address for steal-time accounting of the calling virtual hart and
/// enable the SBI implementation’s steal-time information reporting.
///
/// It is not expected for the shared memory to be written by the supervisor-mode software
/// while it is in use for steal-time accounting. However, the SBI implementation MUST not misbehave
/// if a write operation from supervisor-mode software occurs, however, in that case,
/// it MAY leave the shared memory filled with inconsistent data.
///
/// *NOTE:* Not writing to the shared memory when the supervisor-mode software is not runnable
/// avoids unnecessary work and supports repeatable capture of a system image
/// while the supervisor-mode software is suspended.
///
/// # STA Shared Memory Structure
///
/// | Name      | Offset | Size | Description
/// |:----------|:-------|:-----|:------------
/// | `sequence`  | 0      | 4    | The SBI implementation MUST increment this field to an odd value before writing the `steal` field, and increment it again to an even value after writing `steal` (i.e. an odd sequence number indicates an in-progress update). The SBI implementation SHOULD ensure that the sequence field remains odd for only very short periods of time. <br><br> The supervisor-mode software MUST check this field before and after reading the `steal` field, and repeat the read if it is different or odd. <br><br> This sequence field enables the value of the steal field to be read by supervisor-mode software executed in a 32-bit environment.
/// | `flags`     | 4      | 4    | Always zero. <br><br> Future extensions of the SBI call might allow the supervisor-mode software to write to some fields of the shared memory. Such extensions will not be enabled as long as a zero value is used for the flags argument to the SBI call.
/// | `steal`     | 8      | 8    | The amount of time in which this virtual hart was not idle and scheduled out, in nanoseconds. The time during which the virtual hart is idle will not be reported as steal-time.
/// | `preempted` | 16     | 1    | An advisory flag indicating whether the virtual hart which registered this structure is running or not. A non-zero value MAY be written by the SBI implementation if the virtual hart has been preempted (i.e., while the `steal` field is increasing), while a zero value MUST be written before the virtual hart starts to run again. <br><br> This preempted field can, for example, be used by the supervisor-mode software to check if a lock holder has been preempted, and, in that case, disable optimistic spinning.
/// | `pad`       | 17     | 47   | Pad with zeros to a 64-byte boundary.
///
/// # Parameters
///
/// If `shmem` address is not all-ones bitwise, then `shmem` specifies the shared memory
/// physical base address. `shmem` MUST be 64-byte aligned. The size of the shared memory
/// must be 64 bytes. All bytes MUST be set to zero by the SBI implementation before returning
/// from the SBI call.
///
/// If `shmem` address is all-ones bitwise, the SBI implementation will stop reporting
/// steal-time information for the virtual hart.
///
/// The `flags` parameter is reserved for future use and MUST be zero.
///
/// # Return value
///
/// `SbiRet.value` is set to zero, and the possible error codes returned in `SbiRet.error` are shown in the table below:
///
/// | Error code                  | Description
/// |:----------------------------|:---------------------------------
/// | `SbiRet::success()`         | The steal-time shared memory physical base address was set or cleared successfully.
/// | `SbiRet::invalid_param()`   | The `flags` parameter is not zero or the shmem_phys_lo is not 64-byte aligned.
/// | `SbiRet::invalid_address()` | The shared memory pointed to by the `shmem` parameter is not writable or does not satisfy other requirements of STA Shared Memory Structure.
/// | `SbiRet::failed()`          | The request failed for unspecified or unknown other reasons.
///
/// This function is defined in RISC-V SBI Specification chapter 16.1.
#[inline]
pub fn sta_set_shmem(shmem: SharedPtr<[u8; 64]>, flags: usize) -> SbiRet {
    sbi_call_3(
        EID_STA,
        SET_SHMEM,
        shmem.phys_addr_lo(),
        shmem.phys_addr_hi(),
        flags,
    )
}
