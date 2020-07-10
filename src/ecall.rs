//! 这是一个私有模块
//! 它将会处理所有的SBI调用陷入

// 你应该在riscv-rt或其它中断处理函数里，调用这个模块的内容
mod base;

const EXTENSION_BASE: usize = 0x10;

/// You should call this function in your runtime's exception handler.
/// If the incoming exception is caused by `ecall`, 
/// call this function with parameters extracted from trap frame. 
#[inline]
pub fn handle_ecall(extension: usize, function: usize, param: [usize; 4]) -> SbiRet {
    match extension {
        EXTENSION_BASE => base::handle_ecall_base(function, param[0]),
        _ => todo!()
    }
}

/// Returned by handle_ecall function
/// After `handle_ecall` finished, you should save returned `error` in `a0`, and `value` in `a1`.
#[repr(C)]
pub struct SbiRet {
    /// Error number
    pub error: usize,
    /// Result value
    pub value: usize,
}

impl SbiRet {
    pub(crate) fn ok(value: usize) -> SbiRet {
        SbiRet { error: 0, value }
    }
}
