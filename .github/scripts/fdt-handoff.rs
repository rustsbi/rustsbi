//! QEMU RV64 payload for prototyper-fdt-handoff.sh; deliberately excludes firmware.
#![no_std]
#![no_main]

use core::arch::{asm, naked_asm};

#[repr(C, align(4096))]
struct PageTable([usize; 512]);

static mut ROOT: PageTable = PageTable([0; 512]);
static mut RAM: PageTable = PageTable([0; 512]);
#[repr(C, align(16))]
struct Stack([u8; 16384]);
static mut STACK: Stack = Stack([0; 16384]);

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

extern "C" fn main(_hart: usize, dtb: usize) -> ! {
    // This probe runs only on QEMU virt: firmware is below 0x80200000 and the
    // payload, its tables and stack are above it. No DTB bytes are read yet.
    unsafe {
        let root = (&raw mut ROOT).cast::<usize>();
        let ram = (&raw mut RAM).cast::<usize>();
        for i in 0..512 {
            root.add(i).write_volatile(0);
            ram.add(i).write_volatile(0);
        }
        root.write_volatile(0xcf); // identity-map MMIO in the lowest 1 GiB
        root.add(2).write_volatile(((ram as usize >> 12) << 10) | 1);
        // Leave [0x80000000, 0x80200000) completely unmapped.
        for i in 1..512 {
            ram.add(i)
                .write_volatile((((0x80000000 + i * 0x200000) >> 12) << 10) | 0xcf);
        }
        asm!("csrw stvec, {0}", in(reg) fault as *const () as usize);
        asm!(
            "sfence.vma",
            "csrw satp, {0}",
            "sfence.vma",
            in(reg) ((8usize << 60) | (root as usize >> 12)),
        );
        // Old --fdt behavior faults here because dtb points into the excluded range.
        let magic = u32::from_be((dtb as *const u32).read_volatile());
        let size = u32::from_be((dtb as *const u32).add(1).read_volatile()) as usize;
        let expected = if option_env!("FDT_EXPLICIT").is_some() {
            0x8fc00000
        } else {
            0x8fe00000 // QEMU virt, -m 256M, packed source smaller than 2 MiB
        };
        assert_eq!(dtb, expected);
        assert_eq!(magic, 0xd00d_feed);
        assert!((40..=65536).contains(&size));
        let bytes = core::slice::from_raw_parts(dtb as *const u8, size);
        for marker in [b"rustsbi-dtb-test".as_slice(), b"mmode_resv1@".as_slice()] {
            assert!(bytes.windows(marker.len()).any(|window| window == marker));
        }
    }
    finish(true)
}

#[unsafe(link_section = ".text.trap")]
extern "C" fn fault() -> ! {
    finish(false)
}

fn finish(passed: bool) -> ! {
    let message = if passed {
        b"FDT handoff: PASS\n".as_slice()
    } else {
        b"FDT handoff: FAIL\n".as_slice()
    };
    // QEMU virt UART and test finisher remain mapped and accessible in S-mode.
    unsafe {
        for &byte in message {
            while (0x10000005 as *const u8).read_volatile() & 0x20 == 0 {}
            (0x10000000 as *mut u8).write_volatile(byte);
        }
        (0x100000 as *mut u32).write_volatile(if passed { 0x5555 } else { 0x3333 });
        loop {
            asm!("wfi");
        }
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    finish(false)
}
