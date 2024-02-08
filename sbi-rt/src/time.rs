//! Chapter 6. Timer Extension (EID #0x54494D45 "TIME")

use sbi_spec::{
    binary::SbiRet,
    time::{EID_TIME, SET_TIMER},
};

/// Programs the clock for the next event after an absolute time.
///
/// Parameter `stime_value` is in absolute time.
/// This function must clear the pending timer-interrupt bit as well.
///
/// If the supervisor wishes to clear the timer interrupt without scheduling the next timer event,
/// it can either request a timer interrupt infinitely far into the future (i.e., `u64::MAX`),
/// or it can instead mask the timer interrupt by clearing `sie.STIE` CSR bit.
///
/// This function is defined in RISC-V SBI Specification chapter 6.1.
#[inline]
pub fn set_timer(stime_value: u64) -> SbiRet {
    match () {
        #[cfg(target_pointer_width = "32")]
        () => crate::binary::sbi_call_2(
            EID_TIME,
            SET_TIMER,
            stime_value as _,
            (stime_value >> 32) as _,
        ),
        #[cfg(target_pointer_width = "64")]
        () => crate::binary::sbi_call_1(EID_TIME, SET_TIMER, stime_value as _),
    }
}
