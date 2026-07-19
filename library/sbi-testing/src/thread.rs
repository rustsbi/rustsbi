/// 线程上下文。
#[repr(C)]
pub struct Thread {
    sctx: usize,
    x: [usize; 31],
    sepc: usize,
}

#[allow(unused)]
impl Thread {
    /// 创建空白上下文。
    #[inline]
    pub const fn new(sepc: usize) -> Self {
        Self {
            sctx: 0,
            x: [0; 31],
            sepc,
        }
    }

    /// 读取通用寄存器。
    #[inline]
    pub fn x(&self, n: usize) -> usize {
        self.x[n - 1]
    }

    /// 修改通用寄存器。
    #[inline]
    pub fn x_mut(&mut self, n: usize) -> &mut usize {
        &mut self.x[n - 1]
    }

    /// 读取参数寄存器。
    #[inline]
    pub fn a(&self, n: usize) -> usize {
        self.x(n + 10)
    }

    /// 修改参数寄存器。
    #[inline]
    pub fn a_mut(&mut self, n: usize) -> &mut usize {
        self.x_mut(n + 10)
    }

    /// 读取栈指针。
    #[inline]
    pub fn sp(&self) -> usize {
        self.x(2)
    }

    /// 修改栈指针。
    #[inline]
    pub fn sp_mut(&mut self) -> &mut usize {
        self.x_mut(2)
    }

    /// 将 pc 移至下一条指令。
    ///
    /// # Notice
    ///
    /// 假设这一条指令不是压缩版本。
    #[inline]
    pub fn move_next(&mut self) {
        self.sepc = self.sepc.wrapping_add(4);
    }

    /// 执行此线程，并返回 `sstatus`。
    ///
    /// # Safety
    ///
    /// 将修改 `sscratch`、`sepc`、`sstatus` 和 `stvec`。
    #[inline]
    pub unsafe fn execute(&mut self) -> usize {
        unsafe {
            // 设置线程仍在 S 态并打开中断
            let mut sstatus: usize;
            core::arch::asm!("csrr {}, sstatus", out(reg) sstatus);
            const PRIVILEGE_BIT: usize = 1 << 8;
            const INTERRUPT_BIT: usize = 1 << 5;
            sstatus |= PRIVILEGE_BIT | INTERRUPT_BIT;
            // 执行线程
            #[cfg(target_pointer_width = "64")]
            core::arch::asm!(
                "   csrw sscratch, {sscratch}
                csrw sepc    , {sepc}
                csrw sstatus , {sstatus}
                addi sp, sp, -8
                sd   ra, (sp)
                call {execute_naked}
                ld   ra, (sp)
                addi sp, sp,  8
                csrr {sepc}   , sepc
                csrr {sstatus}, sstatus
            ",
                sscratch      = in(reg) self,
                sepc          = inlateout(reg) self.sepc,
                sstatus       = inlateout(reg) sstatus,
                execute_naked = sym execute_naked,
            );
            #[cfg(target_pointer_width = "32")]
            core::arch::asm!(
                "   csrw sscratch, {sscratch}
                csrw sepc    , {sepc}
                csrw sstatus , {sstatus}
                addi sp, sp, -4
                sw   ra, (sp)
                call {execute_naked}
                lw   ra, (sp)
                addi sp, sp,  4
                csrr {sepc}   , sepc
                csrr {sstatus}, sstatus
            ",
                sscratch      = in(reg) self,
                sepc          = inlateout(reg) self.sepc,
                sstatus       = inlateout(reg) sstatus,
                execute_naked = sym execute_naked,
            );
            sstatus
        }
    }
}

/// 线程切换核心部分。
///
/// 通用寄存器压栈，然后从预存在 `sscratch` 里的上下文指针恢复线程通用寄存器。
///
/// # Safety
///
/// 裸函数。
macro_rules! define_execute_naked {
    ($store:literal, $load:literal, $bytes:literal) => {
        #[unsafe(naked)]
        unsafe extern "C" fn execute_naked() {
            core::arch::naked_asm!(concat!(
                r"  .altmacro
        .macro SAVE n
            ",
                $store,
                r" x\n, \n*",
                $bytes,
                r"(sp)
        .endm
        .macro SAVE_ALL
            ",
                $store,
                " x1, 1*",
                $bytes,
                r"(sp)
            .set n, 3
            .rept 29
                SAVE %n
                .set n, n+1
            .endr
        .endm

        .macro LOAD n
            ",
                $load,
                r" x\n, \n*",
                $bytes,
                r"(sp)
        .endm
        .macro LOAD_ALL
            ",
                $load,
                " x1, 1*",
                $bytes,
                r"(sp)
            .set n, 3
            .rept 29
                LOAD %n
                .set n, n+1
            .endr
        .endm
    ",
                // 位置无关加载
                "   .option push
        .option nopic
    ",
                // 保存调度上下文
                "   addi sp, sp, -32*",
                $bytes,
                r"
        SAVE_ALL
    ",
                // 设置陷入入口
                r"   la   t0, 2f
        csrw stvec, t0
    ",
                // 保存调度上下文地址并切换上下文
                r"   csrr t0, sscratch
        ",
                $store,
                r" sp, (t0)
        mv   sp, t0
    ",
                // 恢复线程上下文
                r"   LOAD_ALL
        ",
                $load,
                " sp, 2*",
                $bytes,
                r"(sp)
    ",
                // 执行线程、陷入并切换上下文。
                r"   sret
        .align 2
    2:  csrrw sp, sscratch, sp
        SAVE_ALL
        csrrw t0, sscratch, sp
        ",
                $store,
                " t0, 2*",
                $bytes,
                r"(sp)
    ",
                // 切换上下文并恢复调度上下文。
                "   ",
                $load,
                r" sp, (sp)
        LOAD_ALL
        addi sp, sp, 32*",
                $bytes,
                r"
        ret
        .option pop
    ",
            ))
        }
    };
}

#[cfg(target_pointer_width = "32")]
define_execute_naked!("sw", "lw", "4");

#[cfg(target_pointer_width = "64")]
define_execute_naked!("sd", "ld", "8");
