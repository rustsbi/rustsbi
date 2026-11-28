//! Machine IMSIC interrupt-file operations under local IRQ exclusion.

use crate::{
    Error, Result,
    irq::{self, LocalIrqDisabled},
};
use csr::{
    Claim, Read,
    standard::aia::{Eidelivery, Eithreshold, Iid, ImsicRegister, imsic, machine::Mtopei},
};
use spin::Once;

/// The interrupt identity reserved for IPIs and the capacity of each machine interrupt file.
pub struct Imsic {
    iid: Iid,
    num_iids: u16,
}

static IMSIC: Once<Imsic> = Once::new();

/// Publishes the machine IMSIC configuration after probing this hart's `mtopei`.
///
/// Call this before publishing an IMSIC-backed IPI service. The platform
/// supplies the number of implemented identities in each machine file.
///
/// After successful registration, further calls return [`Error::AlreadyInitialized`].
/// During registration, [`Error::InvalidArgs`] indicates that `iid` exceeds
/// `num_iids` or that the count is outside the [IMSIC] encoding (63 through
/// 2047, one less than a multiple of 64). [`Error::NotReady`] indicates
/// missing trap recovery and [`Error::NotSupported`] an unreadable `mtopei`.
///
/// [IMSIC]: https://docs.riscv.org/reference/aia/v1.0/IMSIC.html
pub fn init(iid: Iid, num_iids: u16) -> Result<()> {
    let mut initialized = false;
    IMSIC.try_call_once(|| -> Result<Imsic> {
        if !(MIN_IID_COUNT..=MAX_IID_COUNT).contains(&num_iids)
            || num_iids % IID_COUNT_GROUP != IID_COUNT_GROUP - 1
            || iid.number() > num_iids
        {
            return Err(Error::InvalidArgs);
        }
        irq::with_local_disabled(Imsic::detect)?;
        initialized = true;
        Ok(Imsic { iid, num_iids })
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
        if !crate::recovery::is_ready() {
            return Err(Error::NotReady);
        }
        // SAFETY: is_ready checked the recovery context. _irq excludes local reentry.
        unsafe { Mtopei::try_read() }
            .map(|_| ())
            .map_err(|_| Error::NotSupported)
    }

    pub(super) fn init_hart(&self, irq: &LocalIrqDisabled) -> Result<()> {
        Self::detect(irq)?;
        // RV32 uses each selector; RV64 uses every other selector.
        let selector_stride = usize::BITS as usize / IID_PER_REGISTER;
        // Identity zero is reserved, but still occupies a register bit.
        let register_count = (usize::from(self.num_iids) + 1).div_ceil(IID_PER_REGISTER);
        let identity = self.iid.number() as usize;
        let word = identity / usize::BITS as usize;

        let bitmaps = (0..register_count).step_by(selector_stride).flat_map(|index| {
            let enable = if index == word * selector_stride {
                1usize << (identity % usize::BITS as usize)
            } else {
                0
            };
            [(ImsicRegister::Pending(index as u8), 0), (ImsicRegister::Enable(index as u8), enable)]
        });
        let writes = [
            (ImsicRegister::Delivery, Eidelivery::DISABLED),
            (ImsicRegister::Threshold, Eithreshold::ALL),
        ]
        .into_iter()
        .chain(bitmaps)
        .chain([(ImsicRegister::Delivery, Eidelivery::ENABLED)]);
        // SAFETY: Startup owns the detected file. Its size bounds every selector,
        // and the IRQ context excludes competing accesses for the entire sequence.
        unsafe { imsic::write_machine_registers(writes) }.ok_or(Error::InvalidArgs)
    }

    pub(super) fn claim(&self, _irq: &LocalIrqDisabled) -> bool {
        // SAFETY:
        // 1. This hart's IMSIC is initialized before claims.
        // 2. The IRQ context serializes claims on this hart.
        unsafe { Mtopei::claim() }.iid() == Some(self.iid)
    }
}
