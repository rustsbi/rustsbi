//! SpacemiT P1 PMIC reset driver through the K1 I2C controller.

use crate::driver::reset::ResetDevice;
use crate::platform::mmio::Mmio;

/// Register span through the reset-cycle register at offset `0x18`.
pub(super) const SPAN: usize = 0x1c;

#[derive(Clone, Copy)]
enum Reg {
    Icr,
    Isr,
    Idbr,
    Ircr,
}

impl Reg {
    fn offset(self) -> usize {
        match self {
            Self::Icr => 0x00,
            Self::Isr => 0x04,
            Self::Idbr => 0x0c,
            Self::Ircr => 0x18,
        }
    }
}

const CR_START: u32 = 1 << 0;
const CR_STOP: u32 = 1 << 1;
const CR_ACKNAK: u32 = 1 << 2;
const CR_TB: u32 = 1 << 3;
const CR_UR: u32 = 1 << 10;
const CR_SCLE: u32 = 1 << 13;
const CR_IUE: u32 = 1 << 14;
const CR_GCD: u32 = 1 << 21;
const CR_MSDE: u32 = 1 << 26;
const CR_TRANSFER_MASK: u32 = CR_START | CR_STOP | CR_ACKNAK | CR_TB;

/// Interrupt-status bits of the K1 I2C controller.
pub const SR_ACKNAK: u32 = 1 << 14;
pub const SR_UB: u32 = 1 << 15;
pub const SR_IBB: u32 = 1 << 16;
pub const SR_ALD: u32 = 1 << 18;
pub const SR_ITE: u32 = 1 << 19;
pub const SR_IRF: u32 = 1 << 20;
pub const SR_BED: u32 = 1 << 22;
pub const SR_MSD: u32 = 1 << 26;
pub const SR_RXOV: u32 = 1 << 31;

/// Bus error, receiver overrun, and arbitration loss.
pub const SR_ERR: u32 = SR_BED | SR_RXOV | SR_ALD;
/// Interrupt-status bits that the controller defines as write-one-to-clear.
pub const SR_CLEAR_MASK: u32 = 0xfdfc_0000;

const RCR_SDA_GLITCH_NOFIX: u32 = 1 << 7;
const PMIC_PWR_CTRL2: u8 = 0x7e;
const PMIC_PWR_CTRL2_RST: u8 = 1 << 1;
const PMIC_PWR_CTRL2_SHUTDOWN: u8 = 1 << 2;

/// Bound on status-polling iterations for one controller state transition.
const I2C_WAIT_ITERATIONS: u32 = 100_000;

/// SpacemiT K1 I2C controller transport.
struct K1I2c {
    mmio: Mmio,
}

impl K1I2c {
    fn new(mmio: Mmio) -> Self {
        Self { mmio }
    }

    #[inline]
    fn read(&self, reg: Reg) -> u32 {
        self.mmio.read::<u32>(reg.offset())
    }

    #[inline]
    fn write(&self, reg: Reg, value: u32) {
        self.mmio.write::<u32>(reg.offset(), value)
    }

    fn reset_controller(&self) {
        self.write(Reg::Icr, CR_UR);
        for _ in 0..32 {
            core::hint::spin_loop();
        }
        self.write(Reg::Icr, 0);
    }

    fn clear_status(&self, status: u32) {
        self.write(Reg::Isr, status & SR_CLEAR_MASK);
    }

    fn wait_for(&self, mask: u32) -> Option<u32> {
        for _ in 0..I2C_WAIT_ITERATIONS {
            let status = self.read(Reg::Isr);
            if status & (SR_ERR | SR_ACKNAK) != 0 {
                self.clear_status(status);
                self.reset_controller();
                return None;
            }
            if status & mask != 0 {
                self.clear_status(status);
                return Some(status);
            }
            core::hint::spin_loop();
        }
        self.reset_controller();
        None
    }

    fn prepare(&self) -> bool {
        self.reset_controller();
        let reset_cycle = self.read(Reg::Ircr) | RCR_SDA_GLITCH_NOFIX;
        self.write(Reg::Ircr, reset_cycle);
        self.write(Reg::Icr, CR_GCD | CR_SCLE | CR_MSDE | CR_IUE);
        self.clear_status(self.read(Reg::Isr));

        for _ in 0..I2C_WAIT_ITERATIONS {
            if self.read(Reg::Isr) & (SR_UB | SR_IBB) == 0 {
                return true;
            }
            core::hint::spin_loop();
        }
        self.reset_controller();
        false
    }

    fn disable(&self) {
        self.write(Reg::Icr, self.read(Reg::Icr) & !CR_IUE);
    }

    fn start(&self, device_addr: u8, read: bool) -> bool {
        let address = ((device_addr as u32) << 1) | u32::from(read);
        self.write(Reg::Idbr, address);
        let control = (self.read(Reg::Icr) & !CR_TRANSFER_MASK) | CR_START | CR_TB;
        self.write(Reg::Icr, control);
        self.wait_for(SR_ITE).is_some()
    }

    fn send_byte(&self, value: u8, stop: bool) -> bool {
        self.write(Reg::Idbr, value as u32);
        let mut control = (self.read(Reg::Icr) & !CR_TRANSFER_MASK) | CR_TB;
        if stop {
            control |= CR_STOP;
        }
        self.write(Reg::Icr, control);
        self.wait_for(if stop { SR_MSD } else { SR_ITE }).is_some()
    }

    fn write_reg(&self, device_addr: u8, reg: u8, value: u8) -> bool {
        if !self.prepare() {
            return false;
        }
        let result = self.start(device_addr, false)
            && self.send_byte(reg, false)
            && self.send_byte(value, true);
        self.disable();
        result
    }

    fn read_reg(&self, device_addr: u8, reg: u8) -> Option<u8> {
        if !self.prepare() {
            return None;
        }
        let value = (|| {
            if !self.start(device_addr, false)
                || !self.send_byte(reg, false)
                || !self.start(device_addr, true)
            {
                return None;
            }
            let control = (self.read(Reg::Icr) & !CR_TRANSFER_MASK) | CR_ACKNAK | CR_STOP | CR_TB;
            self.write(Reg::Icr, control);
            let status = self.wait_for(SR_IRF)?;
            let value = self.read(Reg::Idbr) as u8;
            if status & SR_MSD == 0 {
                self.wait_for(SR_MSD)?;
            }
            Some(value)
        })();
        self.disable();
        value
    }
}

/// SpacemiT P1 PMIC reached through a K1 I2C controller.
pub(super) struct P1Pmic {
    i2c: K1I2c,
    pmic_addr: u8,
}

impl P1Pmic {
    /// Wraps an acquired I2C controller register block; `pmic_addr` is the
    /// 7-bit I2C address of the PMIC (guaranteed by discovery).
    pub(super) fn new(i2c: Mmio, pmic_addr: u8) -> Self {
        Self {
            i2c: K1I2c::new(i2c),
            pmic_addr,
        }
    }

    fn set_power_control(&self, bit: u8) -> bool {
        self.i2c
            .read_reg(self.pmic_addr, PMIC_PWR_CTRL2)
            .is_some_and(|current| {
                self.i2c
                    .write_reg(self.pmic_addr, PMIC_PWR_CTRL2, current | bit)
            })
    }

    fn park(&self) -> ! {
        loop {
            riscv::asm::wfi();
        }
    }
}

impl ResetDevice for P1Pmic {
    fn fail(&self, _code: u16) -> ! {
        if !self.set_power_control(PMIC_PWR_CTRL2_RST) {
            error!("P1 PMIC reset transaction failed");
        }
        self.park()
    }

    fn pass(&self) -> ! {
        if !self.set_power_control(PMIC_PWR_CTRL2_SHUTDOWN) {
            error!("P1 PMIC shutdown transaction failed");
        }
        self.park()
    }

    fn reset(&self) -> ! {
        if !self.set_power_control(PMIC_PWR_CTRL2_RST) {
            error!("P1 PMIC reset transaction failed");
        }
        self.park()
    }
}
