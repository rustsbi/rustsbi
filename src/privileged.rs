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
/// # Safety
///
/// This function implictly returns to the program address with the address from `mepc` register.
/// Caller must ensure that the value in `mepc` is a valid program address.
/// Caller should also ensure that `mstatus.MPP` register bits contain valid target privilege level.
///
/// # Example
///
/// ```no_run
/// # use riscv::register::{mepc, mstatus::{self, MPP}, mhartid};
/// # extern "C" fn rust_main(hart_id: usize, opauqe: usize) {
/// # fn read_address_from_boot_media() -> usize { 0 /* */ }
/// # let s_mode_start = read_address_from_boot_media(); // read from boot media
/// unsafe {
///     mepc::write(s_mode_start);
///     mstatus::set_mpp(MPP::Supervisor);
///     rustsbi::enter_privileged(mhartid::read(), opauqe);
/// }
/// # }
/// ```
#[inline]
pub unsafe fn enter_privileged(mhartid: usize, opaque: usize) -> ! {
    match () {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))] // pass cargo test
        () => core::arch::asm!(
        "csrrw  sp, mscratch, sp",
        "mret",
        in("a0") mhartid,
        in("a1") opaque,
        options(nomem, noreturn)
        ),
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        () => {
            let _ = (mhartid, opaque);
            unimplemented!("not RISC-V architecture")
        },
    }
}
