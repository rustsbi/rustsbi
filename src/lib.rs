/*

这个库的功能：
1. 在M特权运行，帮助用户搭建运行时，暴露为SBI接口使用的接口
2. 提供简单的pmp配置
3. 帮助用户搭建设备树

设计应该像积木一样，允许用户自己选择模块，而不是提供一个运行时
建议用户配合riscv-sbi-rt使用

todo：考虑这个库是不是单例的。

*/

#![no_std]
#![feature(naked_functions)] // 未来稳定后去掉

pub mod legacy_stdio;
pub mod ecall;

const SBI_SPEC_MAJOR: usize = 0;
const SBI_SPEC_MINOR: usize = 2;

use legacy_stdio::LegacyStdio;

/// RustSBI instance builder; only one hart should build the instance
pub struct Builder<'b> {
    legacy_stdio: Option<&'b dyn LegacyStdio>,
}

impl<'b> Builder<'b> {
    /// Create a new instance builder
    pub fn new() -> Self {
        Builder {
            legacy_stdio: None
        }
    }

    /// Wrap a stdio handler for legacy `getchar` and `putchar` functions
    pub fn legacy_stdio(mut self, stdio: &'b dyn LegacyStdio) -> Self {
        self.legacy_stdio = Some(stdio);
        self
    }

    /// Build the RustSBI instance
    pub fn build(self) -> Instance<'b> {
        todo!()
    }
}

// todo: 修改API
/// RustSBI instance
pub struct Instance<'a> {
    legacy_stdio: Option<&'a dyn LegacyStdio>,
}

impl<'a> Instance<'a> {
    /// Start the instance; call and start the the supervisor
    pub fn start(&mut self) {
        // 这里如果设定了stdout，可以往里面打印一些字
        // 可以用库的crate feature把这个功能关掉
        todo!()
    }
}
