//! 这个模块将会处理所有的 SBI 调用陷入，你应该在中断处理函数里调用 `handle_ecall`

// §4
mod base;
// §6
mod time;
// §7
mod spi;
// §8
mod rfnc;
// §9
mod hsm;
// §10
mod srst;
// §11
mod pmu;

use crate::{
    ipi::send_ipi_many, reset::legacy_reset, HartMask,
};
#[cfg(feature = "legacy")]
use crate::{legacy_stdio_getchar, legacy_stdio_putchar};
use sbi_spec::{self as spec, binary::SbiRet};

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
    match extension {
        spec::rfnc::EID_RFNC => {
            rfnc::handle_ecall(function, param[0], param[1], param[2], param[3], param[4])
        }
        spec::time::EID_TIME => match () {
            #[cfg(target_pointer_width = "64")]
            () => time::handle_ecall(function, param[0]),
            #[cfg(target_pointer_width = "32")]
            () => time::handle_ecall(function, param[0], param[1]),
        },
        spec::spi::EID_SPI => spi::handle_ecall(function, param[0], param[1]),
        spec::base::EID_BASE => base::handle_ecall(function, param[0]),
        spec::hsm::EID_HSM => hsm::handle_ecall(function, param[0], param[1], param[2]),
        spec::srst::EID_SRST => srst::handle_ecall(function, param[0], param[1]),
        spec::pmu::EID_PMU => match () {
            #[cfg(target_pointer_width = "64")]
            () => pmu::handle_ecall(function, param[0], param[1], param[2], param[3], param[4]),
            #[cfg(target_pointer_width = "32")]
            () => pmu::handle_ecall(
                fu32, param[0], param[1], param[2], param[3], param[4], param[5],
            ),
        },
        // handle legacy callings.
        //
        // legacy 调用不使用 a1 返回值，总把 a1 填回 `SbiRet::value` 来模拟非 legacy 的行为。
        #[cfg(feature = "legacy")]
        spec::legacy::LEGACY_SET_TIMER => {
            match () {
                #[cfg(target_pointer_width = "64")]
                () => crate::timer::set_timer(param[0] as _),
                #[cfg(target_pointer_width = "32")]
                () => crate::timer::set_timer(param[0] as _, param[1] as _),
            };
            SbiRet {
                error: param[0],
                value: param[1],
            }
        }
        #[cfg(feature = "legacy")]
        spec::legacy::LEGACY_CONSOLE_PUTCHAR => {
            legacy_stdio_putchar(param[0] as _);
            SbiRet {
                error: param[0],
                value: param[1],
            }
        }
        #[cfg(feature = "legacy")]
        spec::legacy::LEGACY_CONSOLE_GETCHAR => SbiRet {
            error: legacy_stdio_getchar(),
            value: param[1],
        },
        #[cfg(feature = "legacy")]
        spec::legacy::LEGACY_SEND_IPI => {
            send_ipi_many(unsafe { HartMask::legacy_from_addr(param[0]) });
            SbiRet {
                error: param[0],
                value: param[1],
            }
        }
        #[cfg(feature = "legacy")]
        spec::legacy::LEGACY_SHUTDOWN => legacy_reset(),
        _ => SbiRet::not_supported(),
    }
}

#[cfg(target_pointer_width = "32")]
#[inline]
const fn concat_u32(h: usize, l: usize) -> u64 {
    ((h as u64) << 32) | (l as u64)
}
