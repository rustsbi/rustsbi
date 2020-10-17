/// Enter lower privilege from M code with given SBI parameters.
///
/// Before calling this function, you must write target start address into `mepc` register,
/// and write target privilege into `mstatus` register.
/// Platform binarys should call this function on all harts to enter lower privilege levels
/// after the initialization process is finished.
///
/// After this function is called, the stack pointer register `sp` is swapped with `mscratch`,
/// and a `mret` is called to return to `mepc` address with target privilege.
///
/// # Unsafety
///
/// This function implictly returns to the program address with the address from `mepc` register.
/// Caller must ensure that the value in `mepc` is a valid program address.
/// Caller should also ensure that `mstatus.MPP` register bits contain valid target privilege level.
///
/// # Example
///
/// ```rust
/// unsafe {
///     mepc::write(_s_mode_start as usize);
///     mstatus::set_mpp(MPP::Supervisor);
///     enter_privileged(mhartid::read(), dtb_pa);
/// }
/// ```
#[inline]
pub unsafe fn enter_privileged(mhartid: usize, dtb_pa: usize) -> ! {
    match () {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        () => asm!("
            csrrw   sp, mscratch, sp
            mret
        ", in("a0") mhartid, in("a1") dtb_pa, options(nomem, noreturn)),
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        () => {
            drop(mhartid);
            drop(dtb_pa);
            unimplemented!("not RISC-V instruction set architecture")
        }
    }
}
