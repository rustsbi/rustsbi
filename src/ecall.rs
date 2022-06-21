//! 这个模块将会处理所有的SBI调用陷入
// 你应该在riscv-rt或其它中断处理函数里，调用这个模块的内容

mod base;
mod hsm;
mod ipi;
mod legacy;
mod pmu;
mod rfence;
mod srst;
mod timer;

use sbi_spec as spec;

/// Supervisor environment call handler function
///
/// This function is used by platform runtime to handle environment call `ecall` instruction.
///
/// You should call this function in your runtime's exception handler.
/// If the incoming exception is caused by supervisor `ecall`,
/// call this function with parameters extracted from trap frame.
/// After this function returns, store the return value into `a0` and `a1` parameters.
///
/// This function also adapts to the legacy functions.
/// If the supervisor called any of legacy function, the `a0` and `a1` parameter
/// is transferred to error and value field of `SbiRet` respectively.
/// In this case, implementations should always store the result into `a0` and `a1` in
/// any environment call functions including legacy functions.
///
/// # Example
///
/// A typical usage:
///
/// ```no_run
/// # use riscv::register::{mepc, mcause::{self, Trap, Exception}};
/// # struct TrapFrame { a0: usize, a1: usize, a2: usize, a3: usize,
/// # a4: usize, a5: usize, a6: usize, a7: usize }
/// extern "C" fn rust_handle_exception(ctx: &mut TrapFrame) {
///     if mcause::read().cause() == Trap::Exception(Exception::SupervisorEnvCall) {
///         let params = [ctx.a0, ctx.a1, ctx.a2, ctx.a3, ctx.a4, ctx.a5];
///         let ans = rustsbi::ecall(ctx.a7, ctx.a6, params);
///         ctx.a0 = ans.error;
///         ctx.a1 = ans.value;
///         mepc::write(mepc::read().wrapping_add(4));
///     }
///     // other conditions..
/// }
/// ```
///
/// Do not forget to advance `mepc` by 4 after an ecall is handled.
/// This skips the `ecall` instruction itself which is 4-byte long in all conditions.
#[inline]
pub fn handle_ecall(extension: usize, function: usize, param: [usize; 6]) -> SbiRet {
    // RISC-V SBI requires SBI extension IDs (EIDs) and SBI function IDs (FIDs)
    // are encoded as signed 32-bit integers
    #[cfg(not(target_pointer_width = "32"))]
    if u32::try_from(extension).is_err() {
        return SbiRet::not_supported();
    }
    let function = match u32::try_from(function) {
        Ok(f) => f,
        Err(_) => return SbiRet::not_supported(),
    };
    // process actual environment calls
    match extension {
        spec::rfnc::EID_RFNC => {
            rfence::handle_ecall_rfence(function, param[0], param[1], param[2], param[3], param[4])
        }
        spec::time::EID_TIME => match () {
            #[cfg(target_pointer_width = "64")]
            () => timer::handle_ecall_timer_64(function, param[0]),
            #[cfg(target_pointer_width = "32")]
            () => timer::handle_ecall_timer_32(function, param[0], param[1]),
        },
        spec::spi::EID_SPI => ipi::handle_ecall_ipi(function, param[0], param[1]),
        spec::base::EID_BASE => base::handle_ecall_base(function, param[0]),
        spec::hsm::EID_HSM => hsm::handle_ecall_hsm(function, param[0], param[1], param[2]),
        spec::srst::EID_SRST => srst::handle_ecall_srst(function, param[0], param[1]),
        spec::pmu::EID_PMU => match () {
            #[cfg(target_pointer_width = "64")]
            () => {
                pmu::handle_ecall_pmu_64(function, param[0], param[1], param[2], param[3], param[4])
            }
            #[cfg(target_pointer_width = "32")]
            () => pmu::handle_ecall_pmu_32(
                function, param[0], param[1], param[2], param[3], param[4], param[5],
            ),
        },
        spec::legacy::LEGACY_SET_TIMER => match () {
            #[cfg(target_pointer_width = "64")]
            () => legacy::set_timer_64(param[0]),
            #[cfg(target_pointer_width = "32")]
            () => legacy::set_timer_32(param[0], param[1]),
        }
        .legacy_void(param[0], param[1]),
        spec::legacy::LEGACY_CONSOLE_PUTCHAR => {
            legacy::console_putchar(param[0]).legacy_void(param[0], param[1])
        }
        spec::legacy::LEGACY_CONSOLE_GETCHAR => legacy::console_getchar().legacy_return(param[1]),
        spec::legacy::LEGACY_SEND_IPI => legacy::send_ipi(param[0]).legacy_void(param[0], param[1]),
        spec::legacy::LEGACY_SHUTDOWN => legacy::shutdown().legacy_void(param[0], param[1]),
        _ => SbiRet::not_supported(),
    }
}

/// Call result returned by SBI
///
/// After `handle_ecall` finished, you should save returned `error` in `a0`, and `value` in `a1`.
#[repr(C)] // ensure that return value follows RISC-V SBI calling convention
pub struct SbiRet {
    /// Error number
    pub error: usize,
    /// Result value
    pub value: usize,
}

impl SbiRet {
    /// Return success SBI state with given value.
    #[inline]
    pub fn ok(value: usize) -> SbiRet {
        SbiRet {
            error: spec::binary::RET_SUCCESS,
            value,
        }
    }
    /// The SBI call request failed for unknown reasons.
    #[inline]
    pub fn failed() -> SbiRet {
        SbiRet {
            error: spec::binary::RET_ERR_FAILED,
            value: 0,
        }
    }
    /// SBI call failed due to not supported by target ISA, operation type not supported,
    /// or target operation type not implemented on purpose.
    #[inline]
    pub fn not_supported() -> SbiRet {
        SbiRet {
            error: spec::binary::RET_ERR_NOT_SUPPORTED,
            value: 0,
        }
    }
    /// SBI call failed due to invalid hart mask parameter, invalid target hart id, invalid operation type
    /// or invalid resource index.
    #[inline]
    pub fn invalid_param() -> SbiRet {
        SbiRet {
            error: spec::binary::RET_ERR_INVALID_PARAM,
            value: 0,
        }
    }
    /// SBI call failed for invalid mask start address, not a valid physical address parameter,
    /// or the target address is prohibited by PMP to run in supervisor mode.
    #[inline]
    pub fn invalid_address() -> SbiRet {
        SbiRet {
            error: spec::binary::RET_ERR_INVALID_ADDRESS,
            value: 0,
        }
    }
    /// SBI call failed for the target resource is already available, e.g. the target hart is already
    /// started when caller still request it to start.
    #[inline]
    pub fn already_available() -> SbiRet {
        SbiRet {
            error: spec::binary::RET_ERR_ALREADY_AVAILABLE,
            value: 0,
        }
    }
    /// SBI call failed for the target resource is already started, e.g. target performance counter is started.
    #[inline]
    pub fn already_started() -> SbiRet {
        SbiRet {
            error: spec::binary::RET_ERR_ALREADY_STARTED,
            value: 0,
        }
    }
    /// SBI call failed for the target resource is already stopped, e.g. target performance counter is stopped.
    #[inline]
    pub fn already_stopped() -> SbiRet {
        SbiRet {
            error: spec::binary::RET_ERR_ALREADY_STOPPED,
            value: 0,
        }
    }
    #[inline]
    pub(crate) fn legacy_ok(legacy_value: usize) -> SbiRet {
        SbiRet {
            error: legacy_value,
            value: 0,
        }
    }
    // only used for legacy where a0, a1 return value is not modified
    #[inline]
    pub(crate) fn legacy_void(self, a0: usize, a1: usize) -> SbiRet {
        SbiRet {
            error: a0,
            value: a1,
        }
    }
    #[inline]
    pub(crate) fn legacy_return(self, a1: usize) -> SbiRet {
        SbiRet {
            error: self.error,
            value: a1,
        }
    }
}
