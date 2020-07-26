use super::SbiRet;

const FUNCTION_TIMER_SET_TIMER: usize = 0x0;

#[inline]
#[cfg(target_pointer_width = "64")]
pub fn handle_ecall_timer_64(function: usize, param0: usize) -> SbiRet {
    match function {
        FUNCTION_TIMER_SET_TIMER => set_timer(param0),
        _ => SbiRet::not_supported(),
    }
}

#[inline]
#[cfg(target_pointer_width = "32")]
pub fn handle_ecall_timer_32(function: usize, param0: usize, param1: usize) -> SbiRet {
    match function {
        FUNCTION_TIMER_SET_TIMER => set_timer(param0, param1),
        _ => SbiRet::not_supported(),
    }
}

#[cfg(target_pointer_width = "32")]
#[inline]
fn set_timer(arg0: usize, arg1: usize) -> SbiRet {
    let time_value = (arg0 as u64) + ((arg1 as u64) << 32);
    if crate::timer::set_timer(time_value) {
        SbiRet::ok(0)
    } else {
        // should be probed with probe_extension
        SbiRet::not_supported()
    }
}

#[cfg(target_pointer_width = "64")]
#[inline]
fn set_timer(arg0: usize) -> SbiRet {
    let time_value = arg0 as u64;
    if crate::timer::set_timer(time_value) {
        SbiRet::ok(0)
    } else {
        // should be probed with probe_extension
        SbiRet::not_supported()
    }
}
