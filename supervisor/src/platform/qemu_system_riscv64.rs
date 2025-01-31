use crate::drivers::Uart16550U8;

static CONSOLE: Uart16550U8 = unsafe { Uart16550U8::new(0x10000000) };

struct Console;

impl rcore_console::Console for Console {
    fn put_char(&self, c: u8) {
        unsafe { &*CONSOLE.base }.write(&[c]);
    }
    fn put_str(&self, s: &str) {
        unsafe { &*CONSOLE.base }.write(s.as_bytes());
    }
}

pub fn platform_init() {
    rcore_console::init_console(&Console);
}
