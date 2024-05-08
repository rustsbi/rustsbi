//! Board support, including peripheral and core drivers.

use rustsbi::{Console, Physical, RustSBI, SbiRet};

#[derive(RustSBI)]
#[rustsbi(dynamic)]
pub struct Board<'a> {
    #[rustsbi(console)]
    uart16550: Option<ConsoleUart16550<'a>>,
}

struct ConsoleUart16550<'a> {
    inner: &'a uart16550::Uart16550<u8>,
}

impl<'a> Console for ConsoleUart16550<'a> {
    #[inline]
    fn write(&self, bytes: Physical<&[u8]>) -> SbiRet {
        // TODO verify valid memory range for a `Physical` slice.
        let start = bytes.phys_addr_lo();
        let buf = unsafe { core::slice::from_raw_parts(start as *const u8, bytes.num_bytes()) };
        SbiRet::success(self.inner.write(buf))
    }
    #[inline]
    fn read(&self, _bytes: Physical<&mut [u8]>) -> SbiRet {
        todo!()
    }
    #[inline]
    fn write_byte(&self, byte: u8) -> SbiRet {
        self.inner.write(&[byte]);
        SbiRet::success(0)
    }
}
