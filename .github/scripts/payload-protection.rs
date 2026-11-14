//! QEMU RV64 payload for prototyper-payload-protection.sh; runs as the
//! embedded payload and checks that supervisor-memory SBI calls cannot
//! modify the payload's own storage.
#![no_std]
#![no_main]

use core::arch::naked_asm;

#[repr(C, align(16))]
struct Stack([u8; 16384]);
static mut STACK: Stack = Stack([0; 16384]);

/// STA SET_SHMEM target inside the payload's own storage. Every byte starts
/// as `0x5a` so an unexpected write is observable.
#[repr(C, align(64))]
struct Target([u8; 64]);
static mut TARGET: Target = Target([0x5a; 64]);

#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
unsafe extern "C" fn _start() -> ! {
    naked_asm!(
        "la sp, {stack}",
        "li t0, 16384",
        "add sp, sp, t0",
        "tail {main}",
        stack = sym STACK,
        main = sym main,
    );
}

extern "C" fn main(_hart: usize, _dtb: usize) -> ! {
    // This probe runs at the payload address in S-mode, in bare addressing
    // mode: MMIO, the test finisher and its own storage are physical addresses.
    let target = (&raw const TARGET).cast::<u8>() as usize;
    let error = sta_set_shmem(target, 0, 0);
    let mut preserved = true;
    unsafe {
        for offset in 0..64 {
            if ((target + offset) as *const u8).read_volatile() != 0x5a {
                preserved = false;
            }
        }
    }
    // The payload lies outside supervisor-accessible RAM, so SET_SHMEM must
    // report INVALID_ADDRESS (-5) and leave every payload byte unchanged.
    finish(error == -5 && preserved)
}

/// One SBI call, following the RISC-V SBI calling convention: arguments in
/// a0-a2, FID in a6, EID in a7, error in a0, value in a1.
fn sta_set_shmem(addr_lo: usize, addr_hi: usize, flags: usize) -> isize {
    let error;
    // SAFETY: a standard ecall traps into M-mode and returns with a0 set to
    // the SBI error code. a1 is clobbered by the returned value and is not
    // consumed here.
    unsafe {
        core::arch::asm!(
            "ecall",
            inlateout("a0") addr_lo => error,
            inlateout("a1") addr_hi => _,
            in("a2") flags,
            in("a6") 0usize, // FID: STA SET_SHMEM
            in("a7") 0x5354_41, // EID: STA
        );
    }
    error
}

fn finish(passed: bool) -> ! {
    let message = if passed {
        b"Payload protection: PASS\n".as_slice()
    } else {
        b"Payload protection: FAIL\n".as_slice()
    };
    // QEMU virt UART and test finisher remain mapped and accessible in S-mode.
    unsafe {
        for &byte in message {
            while (0x10000005 as *const u8).read_volatile() & 0x20 == 0 {}
            (0x10000000 as *mut u8).write_volatile(byte);
        }
        (0x100000 as *mut u32).write_volatile(if passed { 0x5555 } else { 0x3333 });
        loop {
            core::arch::asm!("wfi");
        }
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    finish(false)
}
