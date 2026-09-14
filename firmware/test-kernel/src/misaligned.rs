//! Misaligned load/store values and stack-pointer write-back.
//!
//! RAM accesses may complete in hardware (for example QEMU with Zicclsm).
//! These checks prove the resulting values, not that emulation was entered.

use core::arch::asm;

#[repr(align(8))]
struct Buffer([u8; 16]);

pub(crate) fn test() {
    test_loads();
    test_stores();
    test_stack_pointer();
}

macro_rules! check {
    ($what:expr, $got:expr, $want:expr) => {{
        let (got, want) = ($got, $want);
        assert_eq!(got, want, "{}: got {got:#x}, want {want:#x}", $what);
    }};
}

fn test_loads() {
    let mut buffer = Buffer([0u8; 16]);
    let odd = unsafe { buffer.0.as_mut_ptr().add(1) as usize };
    let patterns: [[u8; 8]; 2] = [
        [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88],
        [0xFF, 0xFE, 0xFD, 0xFC, 0xFB, 0xFA, 0xF9, 0xF8],
    ];

    for pattern in patterns {
        for (index, byte) in pattern.iter().enumerate() {
            // SAFETY: `odd` points into the local buffer and each write stays
            // within its 16-byte allocation.
            unsafe { core::ptr::write_volatile((odd + index) as *mut u8, *byte) };
        }
        let (b0, b1, b2, b3) = (
            pattern[0] as usize,
            pattern[1] as usize,
            pattern[2] as usize,
            pattern[3] as usize,
        );
        let half = b0 | b1 << 8;
        let word = b0 | b1 << 8 | b2 << 16 | b3 << 24;
        let doubleword = u64::from_le_bytes(pattern) as usize;
        let (lb, lbu, lh, lhu, lw, lwu, ld): (usize, usize, usize, usize, usize, usize, usize);
        // SAFETY: These instructions intentionally access `odd`, an address
        // one byte past the start of an aligned local buffer.
        unsafe {
            asm!("lb {value}, 0({address})", address = in(reg) odd, value = lateout(reg) lb);
            asm!("lbu {value}, 0({address})", address = in(reg) odd, value = lateout(reg) lbu);
            asm!("lh {value}, 0({address})", address = in(reg) odd, value = lateout(reg) lh);
            asm!("lhu {value}, 0({address})", address = in(reg) odd, value = lateout(reg) lhu);
            asm!("lw {value}, 0({address})", address = in(reg) odd, value = lateout(reg) lw);
            asm!("lwu {value}, 0({address})", address = in(reg) odd, value = lateout(reg) lwu);
            asm!("ld {value}, 0({address})", address = in(reg) odd, value = lateout(reg) ld);
        }
        check!("lb", lb, b0 as i8 as usize);
        check!("lbu", lbu, b0);
        check!("lh", lh, half as i16 as usize);
        check!("lhu", lhu, half);
        check!("lw", lw, word as i32 as usize);
        check!("lwu", lwu, word);
        check!("ld", ld, doubleword);
    }
    println!("[misaligned] load widths pass");
}

fn test_stores() {
    let mut buffer = Buffer([0u8; 16]);
    let odd = unsafe { buffer.0.as_mut_ptr().add(1) as usize };
    let cases: [(&str, usize, u64); 4] = [
        ("sb", 1, 0xAB),
        ("sh", 2, 0xBEEF),
        ("sw", 4, 0xDEAD_BEEF),
        ("sd", 8, 0xAA00_BB11_CC22_DD33),
    ];

    for (what, width, value) in cases {
        for byte in &mut buffer.0 {
            *byte = 0;
        }
        // SAFETY: `odd` points into `buffer`; each instruction writes at most
        // eight bytes and therefore remains inside the allocation.
        unsafe {
            match width {
                1 => {
                    asm!("sb {value}, 0({address})", address = in(reg) odd, value = in(reg) value as usize)
                }
                2 => {
                    asm!("sh {value}, 0({address})", address = in(reg) odd, value = in(reg) value as usize)
                }
                4 => {
                    asm!("sw {value}, 0({address})", address = in(reg) odd, value = in(reg) value as usize)
                }
                _ => {
                    asm!("sd {value}, 0({address})", address = in(reg) odd, value = in(reg) value as usize)
                }
            }
        }
        let bytes = value.to_le_bytes();
        for (index, want) in bytes.iter().take(width).enumerate() {
            // SAFETY: the loop is bounded by the store width.
            let got = unsafe { core::ptr::read_volatile((odd + index) as *const u8) };
            check!(what, got as u64, *want as u64);
        }
    }
    println!("[misaligned] store widths pass");
}

fn test_stack_pointer() {
    let mut buffer = Buffer([0u8; 16]);
    let odd = unsafe { buffer.0.as_mut_ptr().add(1) as usize };
    // A misaligned `ld sp` must write back to the trapped register's real
    // frame slot instead of corrupting supervisor `sscratch`.
    unsafe {
        let sp_save: usize;
        asm!("mv {saved}, sp", saved = out(reg) sp_save);
        for (index, byte) in (sp_save as u64).to_le_bytes().iter().enumerate() {
            core::ptr::write_volatile((odd + index) as *mut u8, *byte);
        }
        asm!(
            ".option push",
            ".option norvc",
            "ld sp, 0({address})",
            ".option pop",
            address = in(reg) odd,
        );
        let got: usize;
        asm!("mv {value}, sp", value = out(reg) got);
        asm!("mv sp, {saved}", saved = in(reg) sp_save);
        check!("ld sp", got, sp_save);
    }
    println!("[misaligned] sp write-back pass");
}
