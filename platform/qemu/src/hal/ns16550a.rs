use core::convert::Infallible;
use core::ptr::{read_volatile, write_volatile};
use embedded_hal::serial::{Read, Write};

pub struct Ns16550a {
    base: usize,
    shift: usize,
}

impl Ns16550a {
    pub fn new(base: usize, shift: usize, clk: u64, baud: u64) -> Self {
        // init process; ref: MeowSBI/utils/uart.rs
        unsafe {
            write_volatile((base + (offsets::LCR << shift)) as *mut u8, 0x80); // DLAB

            let latch = clk / (16 * baud);
            write_volatile((base + (offsets::DLL << shift)) as *mut u8, latch as u8);
            write_volatile(
                (base + (offsets::DLH << shift)) as *mut u8,
                (latch >> 8) as u8,
            );

            write_volatile((base + (offsets::LCR << shift)) as *mut u8, 3); // WLEN8 & !DLAB

            write_volatile((base + (offsets::MCR << shift)) as *mut u8, 0);
            write_volatile((base + (offsets::IER << shift)) as *mut u8, 0);
            write_volatile((base + (offsets::FCR << shift)) as *mut u8, 0x7); // FIFO enable + FIFO reset

            // No interrupt for now
        }
        // init finished
        Self { base, shift }
    }
}

impl Read<u8> for Ns16550a {
    // 其实是可能出错的，overrun啊，这些
    type Error = Infallible;

    fn try_read(&mut self) -> nb::Result<u8, Self::Error> {
        let pending =
            unsafe { read_volatile((self.base + (offsets::LSR << self.shift)) as *const u8) }
                & masks::DR;
        if pending != 0 {
            let word =
                unsafe { read_volatile((self.base + (offsets::RBR << self.shift)) as *const u8) };
            Ok(word)
        } else {
            Err(nb::Error::WouldBlock)
        }
    }
}

impl Write<u8> for Ns16550a {
    type Error = Infallible;

    fn try_write(&mut self, word: u8) -> nb::Result<(), Self::Error> {
        // 写，但是不刷新
        unsafe { write_volatile((self.base + (offsets::THR << self.shift)) as *mut u8, word) };
        Ok(())
    }

    fn try_flush(&mut self) -> nb::Result<(), Self::Error> {
        let pending =
            unsafe { read_volatile((self.base + (offsets::LSR << self.shift)) as *const u8) }
                & masks::THRE;
        if pending != 0 {
            // 发送已经结束了
            Ok(())
        } else {
            // 发送还没有结束，继续等
            Err(nb::Error::WouldBlock)
        }
    }
}

mod offsets {
    pub const RBR: usize = 0x0;
    pub const THR: usize = 0x0;

    pub const IER: usize = 0x1;
    pub const FCR: usize = 0x2;
    pub const LCR: usize = 0x3;
    pub const MCR: usize = 0x4;
    pub const LSR: usize = 0x5;

    pub const DLL: usize = 0x0;
    pub const DLH: usize = 0x1;
}

mod masks {
    pub const THRE: u8 = 1 << 5;
    pub const DR: u8 = 1;
}
