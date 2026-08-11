use core::arch::asm;
use sifive_test_device::SifiveTestDevice;

use crate::sbi::reset::ResetDevice;
pub(crate) const SIFIVETEST_COMPATIBLE: [&str; 1] = ["sifive,test0"];
pub(crate) const P1_PMIC_COMPATIBLE: [&str; 2] = [
    "spacemit,p1",
    // Official OrangePi RV2 U-Boot (orangepi-xunlong/u-boot-orangepi,
    // v2022.10-ky) describes the same PMIC (i2c8 @ 0x41) as "ky,spm8821".
    "ky,spm8821",
];

pub struct SifiveTestDeviceWrap {
    inner: *const SifiveTestDevice,
}

impl SifiveTestDeviceWrap {
    pub fn new(base: usize) -> Self {
        Self {
            inner: base as *const SifiveTestDevice,
        }
    }
}

/// Reset Device: SifiveTestDevice
impl ResetDevice for SifiveTestDeviceWrap {
    #[inline]
    fn fail(&self, code: u16) -> ! {
        unsafe { (*self.inner).fail(code) }
    }

    #[inline]
    fn pass(&self) -> ! {
        unsafe { (*self.inner).pass() }
    }

    #[inline]
    fn reset(&self) -> ! {
        unsafe { (*self.inner).reset() }
    }
}

/// SpacemiT P1 PMIC reset device.
///
/// The P1 PMIC is an I2C-controlled power management IC used with the
/// SpacemiT K1 SoC. Reset is triggered by writing to the PMIC's
/// Power Control Register 2 (0x7e):
/// - Bit 1: Reset request
/// - Bit 2: Shutdown request
///
/// This driver directly accesses the K1's I2C controller registers via
/// MMIO to avoid needing a full I2C framework.
pub struct P1PmicResetWrap {
    /// I2C controller MMIO registers.
    i2c: *const I2cK1Registers,
    /// PMIC I2C address (7-bit).
    pmic_addr: u8,
}

impl P1PmicResetWrap {
    /// Create a new P1 PMIC reset device.
    ///
    /// `i2c_base` is the MMIO base address of the I2C controller.
    /// `pmic_addr` is the 7-bit I2C address of the P1 PMIC (0x41 on the
    /// OrangePi RV2, per its device tree `pmic@41` node).
    pub const fn new(i2c_base: usize, pmic_addr: u8) -> Self {
        Self {
            i2c: i2c_base as *const I2cK1Registers,
            pmic_addr,
        }
    }
}

unsafe impl Send for P1PmicResetWrap {}
unsafe impl Sync for P1PmicResetWrap {}

// SpacemiT I2C controller registers (K1).
// Layout per Linux `drivers/i2c/busses/i2c-k1.c` — the K1 I2C controller is
// SpacemiT-proprietary (ICR/ISR/IDBR/IRCR/IBMR), NOT DesignWare I2C.
/// SpacemiT K1 I2C controller MMIO registers.
#[repr(C)]
pub struct I2cK1Registers {
    /// Control register
    icr: u32,
    /// Status register
    isr: u32,
    _reserved0: u32,
    /// Data buffer register
    idbr: u32,
    _reserved1: u32,
    _reserved2: u32,
    /// Reset cycle counter
    ircr: u32,
    /// Bus monitor register
    ibmr: u32,
}

impl I2cK1Registers {
    /// Read the control register (ICR).
    #[inline]
    fn icr(&self) -> u32 {
        // Safety: `self` points at MMIO registers; reads are volatile so
        // the compiler cannot cache or reorder them.
        unsafe { core::ptr::addr_of!(self.icr).read_volatile() }
    }

    /// Write the control register (ICR).
    #[inline]
    fn set_icr(&self, val: u32) {
        // Safety: `self` points at MMIO registers; writes are volatile so
        // the compiler cannot elide or reorder them.
        unsafe { core::ptr::addr_of!(self.icr).cast_mut().write_volatile(val) }
    }

    /// Read the status register (ISR).
    #[inline]
    fn isr(&self) -> u32 {
        // Safety: see `icr`.
        unsafe { core::ptr::addr_of!(self.isr).read_volatile() }
    }

    /// Write the status register (ISR), clearing W1C status bits.
    #[inline]
    fn set_isr(&self, val: u32) {
        // Safety: see `set_icr`.
        unsafe { core::ptr::addr_of!(self.isr).cast_mut().write_volatile(val) }
    }

    /// Write the data buffer register (IDBR).
    #[inline]
    fn set_idbr(&self, val: u32) {
        // Safety: see `set_icr`.
        unsafe {
            core::ptr::addr_of!(self.idbr)
                .cast_mut()
                .write_volatile(val)
        }
    }
}

// ICR (control) bits
const CR_START: u32 = 1 << 0; // start bit
const CR_STOP: u32 = 1 << 1; // stop bit
const CR_ACKNAK: u32 = 1 << 2; // send ACK(0) or NAK(1)
const CR_TB: u32 = 1 << 3; // transfer byte bit
const CR_UR: u32 = 1 << 10; // unit reset
const CR_RSTREQ: u32 = 1 << 11; // i2c bus reset request
const CR_SCLE: u32 = 1 << 13; // master clock enable
const CR_IUE: u32 = 1 << 14; // unit enable
const CR_GCD: u32 = 1 << 21; // general call disable
const CR_MSDE: u32 = 1 << 26; // master STOP detected enable

// ISR (status) bits
const SR_ACKNAK: u32 = 1 << 14; // ACK/NACK status
const SR_UB: u32 = 1 << 15; // unit busy
const SR_IBB: u32 = 1 << 16; // i2c bus busy
const SR_ALD: u32 = 1 << 18; // arbitration loss detected
const SR_ITE: u32 = 1 << 19; // TX buffer empty
const SR_BED: u32 = 1 << 22; // bus error no ACK/NAK
const SR_MSD: u32 = 1 << 26; // master stop detected

// ISR error summary
const SR_ERR: u32 = SR_BED | SR_ALD;

// P1 PMIC registers (per Linux `drivers/power/reset/spacemit-p1-reboot.c`)
const PMIC_PWR_CTRL2: u8 = 0x7e;
const PMIC_PWR_CTRL2_RST: u8 = 1 << 1; // Reset request
const PMIC_PWR_CTRL2_SHUTDOWN: u8 = 1 << 2; // Shutdown request

/// Maximum spin iterations for each I2C status wait before giving up, so a
/// stuck or absent PMIC cannot hang the reset path indefinitely.
const I2C_WAIT_TIMEOUT: u32 = 100_000;

/// Spin until `cond` holds, or the wait timeout is exhausted.
///
/// Returns `true` if the condition was met in time.
#[inline]
fn spin_until(cond: impl Fn() -> bool) -> bool {
    for _ in 0..I2C_WAIT_TIMEOUT {
        if cond() {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

/// I2C operation: write a byte to a register.
///
/// Uses the K1's SpacemiT I2C controller (per Linux
/// `drivers/i2c/busses/i2c-k1.c`): a single PIO transaction sending
/// START, 7-bit address (write), register byte, value byte, STOP.
///
/// If a status wait times out (e.g. the PMIC is absent or the bus is stuck),
/// the function returns early so the caller falls through to its WFI
/// fallback instead of spinning forever.
///
/// # Safety
///
/// `regs` must point to valid MMIO registers of the I2C controller.
unsafe fn i2c_write_reg(regs: &I2cK1Registers, addr_7bit: u8, reg: u8, val: u8) {
    // Configure controller: disable general call, enable SCL clock output
    // and master-stop detection (PIO subset of spacemit_i2c_init()).
    regs.set_icr(CR_GCD | CR_SCLE | CR_MSDE);

    // Wait for the bus to be idle (unit and bus not busy).
    if !spin_until(|| regs.isr() & (SR_UB | SR_IBB) == 0) {
        return;
    }
    // Clear pending status by writing the read value back (W1C).
    regs.set_isr(regs.isr());

    // START + address byte (write direction): load IDBR, assert START|TB.
    regs.set_idbr(((addr_7bit as u32) & 0x7f) << 1);
    regs.set_icr((regs.icr() & !CR_STOP) | CR_START | CR_TB);

    // Wait for the address byte to be transmitted (TX buffer empty).
    if !spin_until(|| regs.isr() & SR_ITE != 0) {
        return;
    }
    regs.set_isr(regs.isr());

    // Register address byte (not the last byte: no STOP).
    regs.set_idbr(reg as u32);
    regs.set_icr((regs.icr() & !(CR_TB | CR_ACKNAK | CR_STOP | CR_START)) | CR_TB);

    // Wait for the register byte to be transmitted.
    if !spin_until(|| regs.isr() & SR_ITE != 0) {
        return;
    }
    regs.set_isr(regs.isr());

    // Value byte: last byte, terminate the transaction with STOP.
    regs.set_idbr(val as u32);
    regs.set_icr((regs.icr() & !(CR_TB | CR_ACKNAK | CR_STOP | CR_START)) | CR_TB | CR_STOP);

    // Wait for the master STOP (transfer complete) or an error/NACK.
    if !spin_until(|| {
        let s = regs.isr();
        s & (SR_MSD | SR_ERR | SR_ACKNAK) != 0
    }) {
        return;
    }
    regs.set_isr(regs.isr());

    // Disable the controller.
    regs.set_icr(0);
}

impl ResetDevice for P1PmicResetWrap {
    #[inline]
    fn fail(&self, _code: u16) -> ! {
        // P1 PMIC: set reset bit in PWR_CTRL2
        unsafe {
            i2c_write_reg(
                &*self.i2c,
                self.pmic_addr,
                PMIC_PWR_CTRL2,
                PMIC_PWR_CTRL2_RST,
            );
        }
        loop {
            unsafe { asm!("wfi") }
        }
    }

    #[inline]
    fn pass(&self) -> ! {
        // P1 PMIC: set shutdown bit in PWR_CTRL2
        unsafe {
            i2c_write_reg(
                &*self.i2c,
                self.pmic_addr,
                PMIC_PWR_CTRL2,
                PMIC_PWR_CTRL2_SHUTDOWN,
            );
        }
        loop {
            unsafe { asm!("wfi") }
        }
    }

    #[inline]
    fn reset(&self) -> ! {
        // P1 PMIC: set reset bit in PWR_CTRL2
        unsafe {
            i2c_write_reg(
                &*self.i2c,
                self.pmic_addr,
                PMIC_PWR_CTRL2,
                PMIC_PWR_CTRL2_RST,
            );
        }
        loop {
            unsafe { asm!("wfi") }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_offsets_match_i2c_k1() {
        // SpacemiT I2C controller layout (Linux drivers/i2c/busses/i2c-k1.c).
        use core::mem::offset_of;
        assert_eq!(offset_of!(I2cK1Registers, icr), 0x00); // Control register
        assert_eq!(offset_of!(I2cK1Registers, isr), 0x04); // Status register
        assert_eq!(offset_of!(I2cK1Registers, idbr), 0x0c); // Data buffer register
        assert_eq!(offset_of!(I2cK1Registers, ircr), 0x18); // Reset cycle counter
        assert_eq!(offset_of!(I2cK1Registers, ibmr), 0x1c); // Bus monitor register
    }

    #[test]
    fn test_icr_bit_masks_match_i2c_k1() {
        assert_eq!(CR_START, 1 << 0);
        assert_eq!(CR_STOP, 1 << 1);
        assert_eq!(CR_ACKNAK, 1 << 2);
        assert_eq!(CR_TB, 1 << 3);
        assert_eq!(CR_UR, 1 << 10);
        assert_eq!(CR_RSTREQ, 1 << 11);
        assert_eq!(CR_SCLE, 1 << 13);
        assert_eq!(CR_IUE, 1 << 14);
        assert_eq!(CR_GCD, 1 << 21);
        assert_eq!(CR_MSDE, 1 << 26);
    }

    #[test]
    fn test_isr_bit_masks_match_i2c_k1() {
        assert_eq!(SR_ACKNAK, 1 << 14);
        assert_eq!(SR_UB, 1 << 15);
        assert_eq!(SR_IBB, 1 << 16);
        assert_eq!(SR_ALD, 1 << 18);
        assert_eq!(SR_ITE, 1 << 19);
        assert_eq!(SR_BED, 1 << 22);
        assert_eq!(SR_MSD, 1 << 26);
        assert_eq!(SR_ERR, SR_BED | SR_ALD);
    }

    #[test]
    fn test_pmic_registers_match_spacemit_p1() {
        // P1 PMIC registers (Linux drivers/power/reset/spacemit-p1-reboot.c).
        assert_eq!(PMIC_PWR_CTRL2, 0x7e);
        assert_eq!(PMIC_PWR_CTRL2_RST, 1 << 1); // Reset request
        assert_eq!(PMIC_PWR_CTRL2_SHUTDOWN, 1 << 2); // Shutdown request
    }
}
