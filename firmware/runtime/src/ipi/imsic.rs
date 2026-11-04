//! Machine IMSIC interrupt-file operations under local IRQ exclusion.

use crate::{
    Error, Result, boot,
    hart::Detect,
    irq::{self, LocalIrqDisabled},
};
use csr::aia::imsic::{Iid, ImsicRegister};
use csr::aia::machine::{Mireg, Miselect, Mtopei};
use csr::machine;
use spin::Once;

/// Runtime's CSR-side configuration for the machine IMSIC.
///
/// Runtime retains the instance and does not expose a constructor or handle.
pub struct Imsic {
    iid: Iid,
    num_iids: u16,
}

static IMSIC: Once<Imsic> = Once::new();

/// Detects and publishes the machine IMSIC selected by the platform.
///
/// The platform must call this before publishing an IPI service backed by IMSIC.
/// A second initialization returns [`Error::AlreadyInitialized`].
/// Returns [`Error::NotReady`] if Runtime is not ready for a current-hart probe.
/// Returns [`Error::NotSupported`] if `mtopei` cannot be read on this hart.
///
/// `num_iids` must be in `63..=2047` and one less than a multiple of 64;
/// `iid` must not exceed that capacity.
/// Before successful initialization, invalid values return [`Error::InvalidArgs`].
/// See <https://docs.riscv.org/reference/aia/v1.0/IMSIC.html>.
pub fn init(iid: Iid, num_iids: u16) -> Result<()> {
    let config = ((MIN_IID_COUNT..=MAX_IID_COUNT).contains(&num_iids)
        && num_iids % IID_COUNT_GROUP == IID_COUNT_GROUP - 1
        && iid.number() <= num_iids)
        .then_some(Imsic { iid, num_iids })
        .ok_or(Error::InvalidArgs);
    let mut initialized = false;
    IMSIC.try_call_once(|| {
        let imsic = config?;
        irq::with_local_disabled(Imsic::detect)?;
        initialized = true;
        Ok(imsic)
    })?;
    initialized.then_some(()).ok_or(Error::AlreadyInitialized)
}

pub(super) fn get() -> Option<&'static Imsic> {
    IMSIC.get()
}

const MIN_IID_COUNT: u16 = 63;
const MAX_IID_COUNT: u16 = Iid::MAX_NUMBER;
/// IMSIC files implement a count one less than a multiple of 64.
const IID_COUNT_GROUP: u16 = 64;
const IID_PER_REGISTER: usize = 32;

impl Imsic {
    fn detect(_irq: &LocalIrqDisabled) -> Result<()> {
        if !boot::traps_ready() || usize::from(machine::Scratch::read()) != 0 {
            return Err(Error::NotReady);
        }
        // SAFETY:
        // 1. Runtime trap recovery is installed, and `MPRV` and `mscratch` are clear.
        // 2. The IRQ context keeps the probe on the current hart without reentry.
        unsafe { Mtopei::detect() }.map_err(|_| Error::NotSupported)
    }

    pub(super) fn init_hart(&self, irq: &LocalIrqDisabled) -> Result<()> {
        Self::detect(irq)?;
        // RV32 uses each selector; RV64 uses every other selector.
        let selector_stride = usize::BITS as usize / IID_PER_REGISTER;
        // Identity zero is reserved, but still occupies a register bit.
        let register_count = (usize::from(self.num_iids) + 1).div_ceil(IID_PER_REGISTER);
        let identity = self.iid.number() as usize;
        let word = identity / usize::BITS as usize;

        for register in [ImsicRegister::Delivery, ImsicRegister::Threshold] {
            let selector = register.selector().ok_or(Error::InvalidArgs)?;
            // SAFETY:
            // 1. Runtime owns this hart's IMSIC file; `selector` and zero are
            //    valid for configuring it.
            // 2. The IRQ context serializes the selector and data accesses.
            unsafe {
                Miselect::write(selector.into());
                Mireg::write(0);
            }
        }
        for index in (0..register_count).step_by(selector_stride) {
            for register in [
                ImsicRegister::Pending(index as u8),
                ImsicRegister::Enable(index as u8),
            ] {
                let selector = register.selector().ok_or(Error::InvalidArgs)?;
                // SAFETY:
                // 1. Runtime owns this hart's IMSIC file; `selector` and zero
                //    are valid for configuring it.
                // 2. The IRQ context serializes indirect accesses.
                unsafe {
                    Miselect::write(selector.into());
                    Mireg::write(0);
                }
            }
        }
        for (register, value) in [
            (
                ImsicRegister::Enable((word * selector_stride) as u8),
                1usize << (identity % usize::BITS as usize),
            ),
            (ImsicRegister::Delivery, 1),
        ] {
            let selector = register.selector().ok_or(Error::InvalidArgs)?;
            // SAFETY:
            // 1. Runtime owns this hart's IMSIC file; `selector` and `value`
            //    are valid for configuring the firmware IPI.
            // 2. The IRQ context serializes indirect accesses.
            unsafe {
                Miselect::write(selector.into());
                Mireg::write(value);
            }
        }
        Ok(())
    }

    pub(super) fn claim(&self, _irq: &LocalIrqDisabled) -> bool {
        // SAFETY:
        // 1. This hart's IMSIC is initialized before claims.
        // 2. The IRQ context serializes claims on this hart.
        unsafe { Mtopei::claim() }.iid() == Some(self.iid)
    }
}
