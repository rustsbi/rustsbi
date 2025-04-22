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
            // TODO support RV32 instruction set
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
#[unsafe(naked)]
unsafe extern "C" fn execute_naked() {
    core::arch::naked_asm!(
        r"  .altmacro
        .macro SAVE n
            sd x\n, \n*8(sp)
        .endm
        .macro SAVE_ALL
            sd x1, 1*8(sp)
            .set n, 3
            .rept 29
                SAVE %n
                .set n, n+1
            .endr
        .endm

        .macro LOAD n
            ld x\n, \n*8(sp)
        .endm
        .macro LOAD_ALL
            ld x1, 1*8(sp)
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
        "   addi sp, sp, -32*8
        SAVE_ALL
    ",
        // 设置陷入入口
        "   la   t0, 2f
        csrw stvec, t0
    ",
        // 保存调度上下文地址并切换上下文
        "   csrr t0, sscratch
        sd   sp, (t0)
        mv   sp, t0
    ",
        // 恢复线程上下文
        "   LOAD_ALL
        ld   sp, 2*8(sp)
    ",
        // 执行线程
        "   sret",
        // 陷入
        "   .align 2",
        // 切换上下文
        "2: csrrw sp, sscratch, sp",
        // 保存线程上下文
        "   SAVE_ALL
        csrrw t0, sscratch, sp
        sd    t0, 2*8(sp)
    ",
        // 切换上下文
        "   ld sp, (sp)",
        // 恢复调度上下文
        "   LOAD_ALL
        addi sp, sp, 32*8
    ",
        // 返回调度
        "   ret",
        "   .option pop",
    )
}
