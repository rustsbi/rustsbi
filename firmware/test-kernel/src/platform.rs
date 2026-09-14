//! Device-tree discovery and the test-kernel console.

use core::ptr::null;

use uart16550::Uart16550;

/// Platform values needed by the test harness after boot discovery.
pub(crate) struct BoardInfo {
    pub(crate) smp: usize,
    pub(crate) frequency: u64,
}

/// Discovers the board and installs the UART-backed global console.
pub(crate) fn initialize(dtb_pa: usize) -> BoardInfo {
    let discovered = DeviceTreeInfo::parse(dtb_pa);
    // The UART address comes from the platform DTB and is valid for the
    // lifetime of the bare-metal test kernel.
    unsafe { UART = Uart16550Map(discovered.uart as _) };
    rcore_console::init_console(&Console);
    rcore_console::set_log_level(option_env!("LOG"));
    BoardInfo {
        smp: discovered.smp,
        frequency: discovered.frequency,
    }
}

/// Values read directly from the device tree, including the UART address
/// needed only while installing the test-kernel console.
struct DeviceTreeInfo {
    smp: usize,
    frequency: u64,
    uart: usize,
}

impl DeviceTreeInfo {
    fn parse(dtb_pa: usize) -> Self {
        use dtb_walker::{Dtb, DtbObj, HeaderError as E, Property, Str, WalkOperation::*};

        let mut board = Self {
            smp: 0,
            frequency: 0,
            uart: 0,
        };
        unsafe {
            Dtb::from_raw_parts_filtered(dtb_pa as _, |error| {
                matches!(error, E::Misaligned(4) | E::LastCompVersion(_))
            })
        }
        .unwrap()
        .walk(|ctx, obj| match obj {
            DtbObj::SubNode { name } => {
                if ctx.is_root() && (name == Str::from("cpus") || name == Str::from("soc")) {
                    StepInto
                } else if ctx.name() == Str::from("cpus") && name.starts_with("cpu@") {
                    board.smp += 1;
                    StepOver
                } else if ctx.name() == Str::from("soc")
                    && (name.starts_with("uart") || name.starts_with("serial"))
                {
                    StepInto
                } else {
                    StepOver
                }
            }
            DtbObj::Property(Property::Reg(mut reg)) => {
                if ctx.name().starts_with("uart") || ctx.name().starts_with("serial") {
                    board.uart = reg.next().unwrap().start;
                }
                StepOut
            }
            DtbObj::Property(Property::General { name, value }) => {
                if ctx.name() == Str::from("cpus") && name == Str::from("timebase-frequency") {
                    board.frequency = match *value {
                        [a, b, c, d] => u32::from_be_bytes([a, b, c, d]) as _,
                        [a, b, c, d, e, f, g, h] => u64::from_be_bytes([a, b, c, d, e, f, g, h]),
                        _ => unreachable!(),
                    };
                }
                StepOver
            }
            DtbObj::Property(_) => StepOver,
        });
        board
    }
}

struct Console;

static mut UART: Uart16550Map = Uart16550Map(null());

/// A UART register block represented as a raw pointer because the device
/// address is discovered at runtime from the board's device tree.
struct Uart16550Map(*const Uart16550<u8>);

unsafe impl Sync for Uart16550Map {}

impl Uart16550Map {
    #[inline]
    fn get(&self) -> &Uart16550<u8> {
        // SAFETY: `initialize` publishes the board-provided MMIO address
        // before any console output is emitted.
        unsafe { &*self.0 }
    }
}

impl rcore_console::Console for Console {
    #[inline]
    fn put_char(&self, c: u8) {
        // SAFETY: console output occurs after `initialize` installs UART.
        unsafe { UART.get().write(core::slice::from_ref(&c)) };
    }

    #[inline]
    fn put_str(&self, text: &str) {
        // SAFETY: console output occurs after `initialize` installs UART.
        unsafe { UART.get().write(text.as_bytes()) };
    }
}
