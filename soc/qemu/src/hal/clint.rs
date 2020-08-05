// 这部分其实是运行时提供的，不应该做到实现库里面

pub struct Clint {
    base: usize,
}

impl Clint {
    pub fn new(base: *mut u8) -> Clint {
        Clint {
            base: base as usize,
        }
    }

    pub fn get_mtime(&self) -> u64 {
        unsafe {
            let base = self.base as *mut u8;
            core::ptr::read_volatile(base.add(0xbff8) as *mut u64)
        }
    }

    pub fn set_timer(&mut self, hart_id: usize, instant: u64) {
        unsafe {
            let base = self.base as *mut u8;
            core::ptr::write_volatile((base.offset(0x4000) as *mut u64).add(hart_id), instant);
        }
    }

    pub fn send_soft(&mut self, hart_id: usize) {
        unsafe {
            let base = self.base as *mut u8;
            core::ptr::write_volatile((base as *mut u32).add(hart_id), 1);
        }
    }

    // pub fn clear_soft(&mut self, hart_id: usize) {
    //     unsafe {
    //         let base = self.base as *mut u8;
    //         core::ptr::write_volatile((base as *mut u32).add(hart_id), 0);
    //     }
    // }
}

use rustsbi::{HartMask, Ipi, Timer};

impl Ipi for Clint {
    fn max_hart_id(&self) -> usize {
        let ans: usize;
        unsafe {
            asm!("
                lui     {ans}, %hi(_max_hart_id)
                add     {ans}, {ans}, %lo(_max_hart_id)
            ", ans = out(reg) ans)
        };
        ans
    }

    fn send_ipi_many(&mut self, hart_mask: HartMask) {
        for i in 0..=self.max_hart_id() {
            if hart_mask.has_bit(i) {
                self.send_soft(i);
            }
        }
    }
}

impl Timer for Clint {
    fn set_timer(&mut self, time_value: u64) {
        let this_mhartid = riscv::register::mhartid::read();
        self.set_timer(this_mhartid, time_value);
    }
}
