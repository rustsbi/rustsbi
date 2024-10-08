use core::fmt::{self, Write};
use rustsbi::{Console, Physical, SbiRet};
use spin::Mutex;
use crate::board::SBI_IMPL;

pub trait ConsoleDevice {
    fn read(&self, buf: &mut [u8]) -> usize;
    fn write(&self, buf: &[u8]) -> usize;
}

pub struct SbiConsole<'a, T: ConsoleDevice> {
    inner: &'a Mutex<T>,
}

impl<'a, T: ConsoleDevice> SbiConsole<'a, T> {
    pub fn new(inner: &'a Mutex<T>) -> Self {
        Self { inner }
    }

    #[inline]
    pub fn putchar(&mut self, c: usize) -> usize {
        self.write_char(c as u8 as char).unwrap();
        0
    }

    #[inline]
    pub fn getchar(&self) -> usize {
        let mut c = 0u8;
        let console = self.inner.lock();
        loop {
            if console.read(core::slice::from_mut(&mut c)) == 1 {break;}
        }
        c as _
    }
}

impl<'a, T: ConsoleDevice> Console for SbiConsole<'a, T> {
    #[inline]
    fn write(&self, bytes: Physical<&[u8]>) -> SbiRet {
        // TODO verify valid memory range for a `Physical` slice.
        let start = bytes.phys_addr_lo();
        let buf = unsafe { core::slice::from_raw_parts(start as *const u8, bytes.num_bytes()) };
        let bytes_num: usize = self.inner.lock().write(buf);
        SbiRet::success(bytes_num)
    }

    #[inline]
    fn read(&self, bytes: Physical<&mut [u8]>) -> SbiRet {
        // TODO verify valid memory range for a `Physical` slice.
        let start = bytes.phys_addr_lo();
        let buf = unsafe { core::slice::from_raw_parts_mut(start as *mut u8, bytes.num_bytes()) };
        let bytes_num = self.inner.lock().read(buf);
        SbiRet::success(bytes_num)
    }

    #[inline]
    fn write_byte(&self, byte: u8) -> SbiRet {
        self.inner.lock().write(&[byte]);
        SbiRet::success(0)
    }
}


impl<'a, T: ConsoleDevice> fmt::Write for SbiConsole<'a, T> {
    #[inline]
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let mut bytes = s.as_bytes();
        let console = self.inner.lock();
        while !bytes.is_empty() {
            let count = console.write(bytes);
            bytes = &bytes[count..];
        }
        Ok(())
    }
}


#[inline]
pub fn putchar(c: usize) -> usize {
    unsafe { SBI_IMPL.assume_init_mut() }.console.as_mut().unwrap().putchar(c)
}

#[inline]
pub fn getchar() -> usize {
    unsafe { SBI_IMPL.assume_init_mut() }.console.as_mut().unwrap().getchar()
}
