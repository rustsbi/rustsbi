//! SpacemiT K1 I2C controller transport.
//!
//! # References
//!
//! - Reference implementation: [Linux K1 I2C driver](https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/drivers/i2c/busses/i2c-k1.c)
//!   — register layout and status-driven PIO sequence.
//! - Firmware implementation: [OpenSBI SpacemiT I2C driver](https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/lib/utils/i2c/fdt_i2c_spacemit.c)
//!   — controller reset delays and transfer timeout.

use bitflags::bitflags;
use core::mem::{align_of, size_of};
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};
use runtime::timer::{Error as TimerError, Timer};

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

// The controller requires 10 us between reset steps and a 1 ms transfer limit.
const RESET_DELAY_TICKS_DIVISOR: u64 = 100_000;
const TRANSFER_TIMEOUT_TICKS_DIVISOR: u64 = 1_000;

pub(super) struct K1I2cController {
    registers: MmioRegion,
    reset_delay_ticks: u64,
    transfer_timeout_ticks: u64,
}

impl K1I2cController {
    pub(super) fn bind(
        registers: DeviceRegisterRange,
        timebase_frequency_hz: Option<u32>,
        memory: &mut MemoryRegistry,
    ) -> runtime::Result<Self> {
        let frequency = u64::from(
            timebase_frequency_hz
                .filter(|frequency| *frequency != 0)
                .ok_or(runtime::Error::InvalidArgs)?,
        );
        let registers = registers.subrange(0, REGISTER_SPAN)?;
        if !registers.start().is_aligned_to(align_of::<u32>()) {
            return Err(runtime::Error::InvalidArgs);
        }
        Ok(Self {
            registers: memory.acquire_mmio(registers)?,
            // Round up so short delays and timeouts do not end early.
            reset_delay_ticks: frequency.div_ceil(RESET_DELAY_TICKS_DIVISOR),
            transfer_timeout_ticks: frequency.div_ceil(TRANSFER_TIMEOUT_TICKS_DIVISOR),
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

    fn reset(&self, timer: &Timer) -> Result<(), TimerError> {
        let delay = || -> Result<(), TimerError> {
            let start = timer.read_time()?;
            while timer.read_time()?.wrapping_sub(start) < self.reset_delay_ticks {
                core::hint::spin_loop();
            }
            Ok(())
        };
        self.write(Register::Control, Control::empty().bits());
        delay()?;
        self.write(Register::Control, Control::UNIT_RESET.bits());
        delay()?;
        self.write(
            Register::Control,
            (Control::UNIT_ENABLE | Control::SCL_ENABLE).bits(),
        );
        Ok(())
    }

    fn clear_status(&self, status: Status) {
        self.write(Register::Status, (status & CLEARABLE_STATUS).bits());
    }

    fn wait_for_status(&self, timer: &Timer, mask: Status) -> Result<Option<Status>, TimerError> {
        let start = timer.read_time()?;
        loop {
            let status = Status::from_bits_retain(self.read(Register::Status));
            if status.intersects(STATUS_ERRORS | Status::ACK_NAK) {
                self.clear_status(status);
                self.reset(timer)?;
                return Ok(None);
            }
            if status.intersects(mask) {
                self.clear_status(status);
                return Ok(Some(status));
            }
            if timer.read_time()?.wrapping_sub(start) >= self.transfer_timeout_ticks {
                self.reset(timer)?;
                return Ok(None);
            }
            core::hint::spin_loop();
        }
    }

    fn prepare_transfer(&self, timer: &Timer) -> Result<bool, TimerError> {
        self.reset(timer)?;
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

        let start = timer.read_time()?;
        loop {
            let status = Status::from_bits_retain(self.read(Register::Status));
            if !status.intersects(Status::UNIT_BUSY | Status::BUS_BUSY) {
                return Ok(true);
            }
            if timer.read_time()?.wrapping_sub(start) >= self.transfer_timeout_ticks {
                self.reset(timer)?;
                return Ok(false);
            }
            core::hint::spin_loop();
        }
    }

    fn disable(&self) {
        let control = Control::from_bits_retain(self.read(Register::Control));
        self.write(Register::Control, (control - Control::UNIT_ENABLE).bits());
    }

    fn start(
        &self,
        timer: &Timer,
        device: I2cAddress,
        direction: Direction,
    ) -> Result<bool, TimerError> {
        let address = (device.get() << 1) | direction as u8;
        self.write(Register::DataBuffer, u32::from(address));
        let control = Control::from_bits_retain(self.read(Register::Control)) - TRANSFER_CONTROL;
        self.write(
            Register::Control,
            (control | Control::START | Control::TRANSFER_BYTE).bits(),
        );
        Ok(self.wait_for_status(timer, Status::TX_EMPTY)?.is_some())
    }

    fn send_byte(&self, timer: &Timer, value: u8, stop: bool) -> Result<bool, TimerError> {
        self.write(Register::DataBuffer, u32::from(value));
        let mut control = (Control::from_bits_retain(self.read(Register::Control))
            - TRANSFER_CONTROL)
            | Control::TRANSFER_BYTE;
        if stop {
            control |= Control::STOP;
        }
        self.write(Register::Control, control.bits());
        Ok(self
            .wait_for_status(
                timer,
                if stop {
                    Status::MASTER_STOP_DETECTED
                } else {
                    Status::TX_EMPTY
                },
            )?
            .is_some())
    }

    pub(super) fn write_register(
        &self,
        device: I2cAddress,
        register: u8,
        value: u8,
    ) -> Result<bool, TimerError> {
        let timer = Timer::current()?;
        let result = (|| {
            if !self.prepare_transfer(&timer)? {
                return Ok(false);
            }
            Ok(self.start(&timer, device, Direction::Write)?
                && self.send_byte(&timer, register, false)?
                && self.send_byte(&timer, value, true)?)
        })();
        // A failed time read must release the controller just like a transfer
        // failure, while retaining the original timer error for the caller.
        self.disable();
        result
    }

    pub(super) fn read_register(
        &self,
        device: I2cAddress,
        register: u8,
    ) -> Result<Option<u8>, TimerError> {
        let timer = Timer::current()?;
        let value = (|| {
            if !self.prepare_transfer(&timer)?
                || !self.start(&timer, device, Direction::Write)?
                || !self.send_byte(&timer, register, false)?
                || !self.start(&timer, device, Direction::Read)?
            {
                return Ok(None);
            }
            let control = (Control::from_bits_retain(self.read(Register::Control))
                - TRANSFER_CONTROL)
                | Control::ACK_NAK
                | Control::STOP
                | Control::TRANSFER_BYTE;
            self.write(Register::Control, control.bits());
            let Some(status) = self.wait_for_status(&timer, Status::RX_FULL)? else {
                return Ok(None);
            };
            let value = self.read(Register::DataBuffer) as u8;
            if !status.contains(Status::MASTER_STOP_DETECTED)
                && self
                    .wait_for_status(&timer, Status::MASTER_STOP_DETECTED)?
                    .is_none()
            {
                return Ok(None);
            }
            Ok(Some(value))
        })();
        self.disable();
        value
    }
}
