//! SpacemiT K1 I2C controller transport.
//!
//! Reference implementation: the pinned [Linux K1 I2C driver] defines the
//! register layout and status-driven PIO sequence. Compatibility reference:
//! the pinned [OpenSBI SpacemiT I2C driver] defines the firmware reset delays
//! and transfer timeout used here.
//!
//! [Linux K1 I2C driver]: https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/drivers/i2c/busses/i2c-k1.c
//! [OpenSBI SpacemiT I2C driver]: https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/lib/utils/i2c/fdt_i2c_spacemit.c

use bitflags::bitflags;
use core::mem::{align_of, size_of};
use core::time::Duration;
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

use super::I2cAddress;

#[repr(usize)]
#[derive(Clone, Copy)]
enum Register {
    Control = 0x00,
    Status = 0x04,
    DataBuffer = 0x0c,
    ResetCycle = 0x18,
}

impl Register {
    const fn offset(self) -> usize {
        self as usize
    }
}

const REGISTER_SPAN: usize = Register::ResetCycle.offset() + size_of::<u32>();

bitflags! {
    #[derive(Clone, Copy)]
    struct Control: u32 {
        const START = 1 << 0;
        const STOP = 1 << 1;
        const ACK_NAK = 1 << 2;
        const TRANSFER_BYTE = 1 << 3;
        const UNIT_RESET = 1 << 10;
        const SCL_ENABLE = 1 << 13;
        const UNIT_ENABLE = 1 << 14;
        const GENERAL_CALL_DISABLE = 1 << 21;
        const MASTER_STOP_DETECT_ENABLE = 1 << 26;
    }

    #[derive(Clone, Copy)]
    struct Status: u32 {
        const ACK_NAK = 1 << 14;
        const UNIT_BUSY = 1 << 15;
        const BUS_BUSY = 1 << 16;
        const ARBITRATION_LOSS = 1 << 18;
        const TX_EMPTY = 1 << 19;
        const RX_FULL = 1 << 20;
        const GENERAL_CALL_ADDRESS_DETECTED = 1 << 21;
        const BUS_ERROR = 1 << 22;
        const SLAVE_ADDRESS_DETECTED = 1 << 23;
        const SLAVE_STOP_DETECTED = 1 << 24;
        const MASTER_STOP_DETECTED = 1 << 26;
        const TRANSACTION_DONE = 1 << 27;
        const TX_FIFO_EMPTY = 1 << 28;
        const RX_FIFO_HALF_FULL = 1 << 29;
        const RX_FIFO_FULL = 1 << 30;
        const RX_OVERRUN = 1 << 31;
    }

    #[derive(Clone, Copy)]
    struct ResetCycle: u32 {
        const SDA_GLITCH_FILTER_BYPASS = 1 << 7;
    }
}

const TRANSFER_CONTROL: Control = Control::START
    .union(Control::STOP)
    .union(Control::ACK_NAK)
    .union(Control::TRANSFER_BYTE);
const STATUS_ERRORS: Status = Status::BUS_ERROR
    .union(Status::RX_OVERRUN)
    .union(Status::ARBITRATION_LOSS);
const CLEARABLE_STATUS: Status = Status::ARBITRATION_LOSS
    .union(Status::TX_EMPTY)
    .union(Status::RX_FULL)
    .union(Status::GENERAL_CALL_ADDRESS_DETECTED)
    .union(Status::BUS_ERROR)
    .union(Status::SLAVE_ADDRESS_DETECTED)
    .union(Status::SLAVE_STOP_DETECTED)
    .union(Status::MASTER_STOP_DETECTED)
    .union(Status::TRANSACTION_DONE)
    .union(Status::TX_FIFO_EMPTY)
    .union(Status::RX_FIFO_HALF_FULL)
    .union(Status::RX_FIFO_FULL)
    .union(Status::RX_OVERRUN);

#[repr(u8)]
enum Direction {
    Write = 0,
    Read = 1,
}

const CONTROLLER_RESET_DELAY: Duration = Duration::from_micros(10);
const TRANSFER_TIMEOUT: Duration = Duration::from_micros(1_000);
const NANOSECONDS_PER_SECOND: u64 = 1_000_000_000;

#[derive(Clone, Copy)]
struct Timebase(u64);

impl Timebase {
    fn new(frequency_hz: u32) -> Option<Self> {
        (frequency_hz != 0).then_some(Self(u64::from(frequency_hz)))
    }

    fn ticks_for(self, duration: Duration) -> u64 {
        let whole_seconds = duration.as_secs().saturating_mul(self.0);
        let fractional = u64::from(duration.subsec_nanos())
            .saturating_mul(self.0)
            .div_ceil(NANOSECONDS_PER_SECOND);
        whole_seconds.saturating_add(fractional)
    }

    fn elapsed(self, start: u64, duration: Duration) -> bool {
        riscv::register::time::read64().wrapping_sub(start) >= self.ticks_for(duration)
    }

    fn delay(self, duration: Duration) {
        let start = riscv::register::time::read64();
        while !self.elapsed(start, duration) {
            core::hint::spin_loop();
        }
    }
}

pub(super) struct K1I2cController {
    registers: MmioRegion,
    timebase: Timebase,
}

impl K1I2cController {
    pub(super) fn bind(
        registers: DeviceRegisterRange,
        timebase_frequency_hz: Option<u32>,
        memory: &mut MemoryRegistry,
    ) -> runtime::Result<Self> {
        let timebase = Timebase::new(timebase_frequency_hz.ok_or(runtime::Error::InvalidArgs)?)
            .ok_or(runtime::Error::InvalidArgs)?;
        let registers = registers.subrange(0, REGISTER_SPAN)?;
        if !registers.start().is_aligned_to(align_of::<u32>()) {
            return Err(runtime::Error::InvalidArgs);
        }
        Ok(Self {
            registers: memory.acquire_mmio(registers)?,
            timebase,
        })
    }

    #[inline]
    fn read(&self, register: Register) -> u32 {
        self.registers
            .read(register.offset())
            .expect("BUG: K1 I2C register escaped its MMIO window")
    }

    #[inline]
    fn write(&self, register: Register, value: u32) {
        self.registers
            .write(register.offset(), value)
            .expect("BUG: K1 I2C register escaped its MMIO window")
    }

    fn reset(&self) {
        self.write(Register::Control, Control::empty().bits());
        self.timebase.delay(CONTROLLER_RESET_DELAY);
        self.write(Register::Control, Control::UNIT_RESET.bits());
        self.timebase.delay(CONTROLLER_RESET_DELAY);
        self.write(
            Register::Control,
            (Control::UNIT_ENABLE | Control::SCL_ENABLE).bits(),
        );
    }

    fn clear_status(&self, status: Status) {
        self.write(Register::Status, (status & CLEARABLE_STATUS).bits());
    }

    fn wait_for_status(&self, mask: Status) -> Option<Status> {
        let start = riscv::register::time::read64();
        loop {
            let status = Status::from_bits_retain(self.read(Register::Status));
            if status.intersects(STATUS_ERRORS | Status::ACK_NAK) {
                self.clear_status(status);
                self.reset();
                return None;
            }
            if status.intersects(mask) {
                self.clear_status(status);
                return Some(status);
            }
            if self.timebase.elapsed(start, TRANSFER_TIMEOUT) {
                self.reset();
                return None;
            }
            core::hint::spin_loop();
        }
    }

    fn prepare_transfer(&self) -> bool {
        self.reset();
        let reset_cycle = ResetCycle::from_bits_retain(self.read(Register::ResetCycle))
            | ResetCycle::SDA_GLITCH_FILTER_BYPASS;
        self.write(Register::ResetCycle, reset_cycle.bits());
        self.write(
            Register::Control,
            (Control::GENERAL_CALL_DISABLE
                | Control::SCL_ENABLE
                | Control::MASTER_STOP_DETECT_ENABLE
                | Control::UNIT_ENABLE)
                .bits(),
        );
        self.clear_status(Status::from_bits_retain(self.read(Register::Status)));

        let start = riscv::register::time::read64();
        loop {
            let status = Status::from_bits_retain(self.read(Register::Status));
            if !status.intersects(Status::UNIT_BUSY | Status::BUS_BUSY) {
                return true;
            }
            if self.timebase.elapsed(start, TRANSFER_TIMEOUT) {
                self.reset();
                return false;
            }
            core::hint::spin_loop();
        }
    }

    fn disable(&self) {
        let control = Control::from_bits_retain(self.read(Register::Control));
        self.write(Register::Control, (control - Control::UNIT_ENABLE).bits());
    }

    fn start(&self, device: I2cAddress, direction: Direction) -> bool {
        let address = (device.get() << 1) | direction as u8;
        self.write(Register::DataBuffer, u32::from(address));
        let control = Control::from_bits_retain(self.read(Register::Control)) - TRANSFER_CONTROL;
        self.write(
            Register::Control,
            (control | Control::START | Control::TRANSFER_BYTE).bits(),
        );
        self.wait_for_status(Status::TX_EMPTY).is_some()
    }

    fn send_byte(&self, value: u8, stop: bool) -> bool {
        self.write(Register::DataBuffer, u32::from(value));
        let mut control = (Control::from_bits_retain(self.read(Register::Control))
            - TRANSFER_CONTROL)
            | Control::TRANSFER_BYTE;
        if stop {
            control |= Control::STOP;
        }
        self.write(Register::Control, control.bits());
        self.wait_for_status(if stop {
            Status::MASTER_STOP_DETECTED
        } else {
            Status::TX_EMPTY
        })
        .is_some()
    }

    pub(super) fn write_register(&self, device: I2cAddress, register: u8, value: u8) -> bool {
        if !self.prepare_transfer() {
            return false;
        }
        let result = self.start(device, Direction::Write)
            && self.send_byte(register, false)
            && self.send_byte(value, true);
        self.disable();
        result
    }

    pub(super) fn read_register(&self, device: I2cAddress, register: u8) -> Option<u8> {
        if !self.prepare_transfer() {
            return None;
        }
        let value = (|| {
            if !self.start(device, Direction::Write)
                || !self.send_byte(register, false)
                || !self.start(device, Direction::Read)
            {
                return None;
            }
            let control = (Control::from_bits_retain(self.read(Register::Control))
                - TRANSFER_CONTROL)
                | Control::ACK_NAK
                | Control::STOP
                | Control::TRANSFER_BYTE;
            self.write(Register::Control, control.bits());
            let status = self.wait_for_status(Status::RX_FULL)?;
            let value = self.read(Register::DataBuffer) as u8;
            if !status.contains(Status::MASTER_STOP_DETECTED) {
                self.wait_for_status(Status::MASTER_STOP_DETECTED)?;
            }
            Some(value)
        })();
        self.disable();
        value
    }
}
