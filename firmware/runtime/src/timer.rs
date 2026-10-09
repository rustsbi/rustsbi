//! Current-hart timer deadlines and interrupt delivery.
//!
//! Runtime selects Sstc or a published platform comparator independently on
//! each hart. It owns comparator cancellation, MTIE, and the MTIP-to-STIP
//! transport.
//! Platform devices only provide MMIO operations and counters.

#![forbid(unsafe_code)]

use alloc::boxed::Box;
use core::fmt;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use spin::{Mutex, Once};

use crate::csr::{Csr64, Mie, Mip, Readable, Stimecmp, Time, Value, Writable};
#[cfg(target_pointer_width = "64")]
use crate::csr::{EnvironmentConfig, Menvcfg};
#[cfg(target_pointer_width = "32")]
use crate::csr::{EnvironmentConfigHigh, MenvcfgHigh, StimecmpHigh};
use crate::hart::HartId;

// Timer comparison writes retain their own compare-safe RV32 sequence.
impl Stimecmp {
    fn write_time(value: u64) -> Result<(), crate::trap::Error> {
        #[cfg(target_pointer_width = "64")]
        {
            Self::write(value as usize)
        }
        #[cfg(target_pointer_width = "32")]
        {
            Self::write(usize::MAX)?;
            StimecmpHigh::write((value >> 32) as usize)?;
            Self::write(value as usize)
        }
    }
}

static PLATFORM_TIMER: Once<Option<PlatformTimer>> = Once::new();
static REQUIRED: AtomicBool = AtomicBool::new(false);

const SSTC_UNKNOWN: u8 = 0;
const SSTC_ABSENT: u8 = 1;
const SSTC_PRESENT: u8 = 2;

const MODE_UNINITIALIZED: u8 = 0;
const MODE_UNAVAILABLE: u8 = 1;
const MODE_PLATFORM: u8 = 2;
const MODE_SSTC: u8 = 3;

struct HartTimer {
    sstc: AtomicU8,
    mode: AtomicU8,
}

static HARTS: Once<Box<[HartTimer]>> = Once::new();

fn state(hart: HartId) -> &'static HartTimer {
    &HARTS.call_once(|| {
        HartId::all()
            .map(|_| HartTimer {
                sstc: AtomicU8::new(SSTC_UNKNOWN),
                mode: AtomicU8::new(MODE_UNINITIALIZED),
            })
            .collect()
    })[hart.index()]
}

/// A current-hart timer operation failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// The current hart is not in the published boot topology.
    InvalidHartId,
    /// The capability belongs to a different hart than the one executing it.
    WrongHart,
    /// Platform binding or this hart's timer transport has not been initialized.
    NotInitialized,
    /// Neither Sstc nor a platform comparator is available.
    Unavailable,
    /// An architectural timer access faulted.
    Csr(crate::trap::Error),
    /// The hardware did not retain `menvcfg.STCE`.
    SstcDisabled,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHartId => formatter.write_str("hart ID is not in the boot topology"),
            Self::WrongHart => formatter.write_str("timer capability belongs to another hart"),
            Self::NotInitialized => {
                formatter.write_str("timer binding or current hart transport is not initialized")
            }
            Self::Unavailable => formatter.write_str("current hart has no timer comparator"),
            Self::Csr(error) => write!(formatter, "architectural timer access failed: {error}"),
            Self::SstcDisabled => {
                formatter.write_str("hardware did not enable supervisor Sstc access")
            }
        }
    }
}

/// A readable platform time counter, independent of its timer comparator.
pub trait TimeSource: Send + Sync {
    /// Reads the complete counter, consistently across rollover on RV32.
    fn read_time(&self) -> u64;

    /// Reads a direct device counter word without probing the architecture CSR.
    #[inline]
    fn read_time_low(&self) -> usize {
        self.read_time() as usize
    }

    /// Reads a direct device counter high word on RV32.
    #[cfg(target_pointer_width = "32")]
    #[inline]
    fn read_time_high(&self) -> usize {
        (self.read_time() >> 32) as usize
    }
}

/// A platform timer comparator with an optional time source.
pub trait TimerDevice: Send + Sync {
    /// Programs the comparison value for `hart`.
    ///
    /// Runtime serializes comparator writes, including RV32 multi-word updates.
    fn set_deadline(&self, hart: HartId, deadline: u64);

    /// Returns the optional time source that Runtime snapshots during installation.
    fn time_source(&self) -> Option<&dyn TimeSource> {
        None
    }

    /// Acknowledges expiry after MTIE is masked, retaining cancellation as the default.
    fn acknowledge(&self, hart: HartId) {
        self.set_deadline(hart, u64::MAX);
    }
}

/// Platform timer binding with serialized comparator transactions.
///
/// Counter reads stay unlocked.
struct PlatformTimer {
    device: &'static dyn TimerDevice,
    time_source: Option<&'static dyn TimeSource>,
    programming: Mutex<()>,
}

impl PlatformTimer {
    fn set_deadline(&self, hart: HartId, deadline: u64) {
        let _guard = self.programming.lock();
        self.device.set_deadline(hart, deadline);
    }

    fn acknowledge(&self, hart: HartId) {
        let _guard = self.programming.lock();
        self.device.acknowledge(hart);
    }
}

/// Current-hart timer capability issued by Runtime.
///
/// The capability binds the current hart's cached Sstc probe and selected
/// transport. It can be obtained before trap initialization for discovery and
/// time reads; deadlines require an initialized transport. It cannot be sent
/// to another hart or shared between harts.
pub struct Timer {
    hart: HartId,
    state: &'static HartTimer,
    _not_send_sync: PhantomData<*mut ()>,
}

impl Timer {
    /// Validates the executing hart and obtains its timer capability.
    pub fn current() -> Result<Self, Error> {
        let hart = HartId::current().map_err(|_| Error::InvalidHartId)?;
        Ok(Self {
            hart,
            state: state(hart),
            _not_send_sync: PhantomData,
        })
    }

    /// Publishes the optional platform comparator once during boot.
    ///
    /// Sstc does not require a platform device. Publish before
    /// [`crate::trap::init()`].
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::AlreadyInitialized`] after the first installation.
    pub fn install(device: Option<&'static dyn TimerDevice>) -> crate::Result<()> {
        let mut installed = false;
        PLATFORM_TIMER.call_once(|| {
            installed = true;
            device.map(|device| PlatformTimer {
                device,
                time_source: device.time_source(),
                programming: Mutex::new(()),
            })
        });
        if installed {
            Ok(())
        } else {
            Err(crate::Error::AlreadyInitialized)
        }
    }

    /// Requires a deadline source on every hart before trap state is published.
    ///
    /// Firmware calls this before publishing SBI TIME. The current hart is
    /// checked immediately; secondary harts are checked during initialization.
    pub fn require_available(&self) -> Result<(), Error> {
        self.check_current()?;
        let timer = PLATFORM_TIMER.get().ok_or(Error::NotInitialized)?;
        if !self.supports_sstc()? && timer.is_none() {
            return Err(Error::Unavailable);
        }
        REQUIRED.store(true, Ordering::Release);
        Ok(())
    }

    /// Reads this hart's 64-bit time counter before or after trap initialization.
    ///
    /// RV32 CSR reads are consistent across counter rollover. A missing
    /// architectural counter falls back to the published platform counter;
    /// other CSR errors retain their original cause.
    pub fn read_time(&self) -> Result<u64, Error> {
        self.check_current()?;
        riscv::interrupt::machine::free(|| match Time::read64() {
            Ok(value) => Ok(value),
            Err(crate::trap::Error::UnsupportedInstruction) => self
                .platform_time_source()
                .map(TimeSource::read_time)
                .ok_or(Error::Csr(crate::trap::Error::UnsupportedInstruction)),
            Err(error) => Err(Error::Csr(error)),
        })
    }

    /// Probes this hart's Sstc comparator once and caches its presence.
    ///
    /// An unsupported CSR establishes absence. Other architectural faults are
    /// returned without converting them to an absent extension.
    pub fn supports_sstc(&self) -> Result<bool, Error> {
        self.check_current()?;
        match self.state.sstc.load(Ordering::Acquire) {
            SSTC_PRESENT => Ok(true),
            SSTC_ABSENT => Ok(false),
            _ => {
                let supported = Stimecmp::read_optional().map_err(Error::Csr)?.is_some();
                #[cfg(target_pointer_width = "32")]
                let supported =
                    supported && StimecmpHigh::read_optional().map_err(Error::Csr)?.is_some();
                self.state.sstc.store(
                    if supported { SSTC_PRESENT } else { SSTC_ABSENT },
                    Ordering::Release,
                );
                Ok(supported)
            }
        }
    }

    fn check_current(&self) -> Result<(), Error> {
        let hart = HartId::current().map_err(|_| Error::InvalidHartId)?;
        if hart != self.hart {
            return Err(Error::WrongHart);
        }
        Ok(())
    }

    fn mode(&self) -> Result<u8, Error> {
        match self.state.mode.load(Ordering::Acquire) {
            MODE_UNINITIALIZED => Err(Error::NotInitialized),
            mode => Ok(mode),
        }
    }

    /// Programs an absolute timer deadline on this hart.
    ///
    /// Sstc drives STIP directly. A platform comparator clears a previous STIP
    /// and enables MTIE after programming the deadline. `u64::MAX` cancels
    /// delivery through [`Self::cancel_current`].
    pub fn set_deadline(&self, deadline: u64) -> Result<(), Error> {
        self.check_current()?;
        if deadline == u64::MAX {
            return self.cancel_current();
        }
        riscv::interrupt::machine::free(|| {
            match self.mode()? {
                MODE_SSTC => Stimecmp::write_time(deadline).map_err(Error::Csr)?,
                MODE_PLATFORM => {
                    let device = self
                        .platform_timer()
                        .expect("BUG: platform timer mode without a device");
                    device.set_deadline(self.hart, deadline);
                    Mip::clear_bits(Mip::SUPERVISOR_TIMER);
                    Mie::set_bits(Mie::MACHINE_TIMER);
                }
                MODE_UNAVAILABLE => return Err(Error::Unavailable),
                _ => unreachable!("BUG: invalid current hart timer mode"),
            }
            Ok(())
        })
    }

    /// Cancels this hart's comparator and pending supervisor delivery.
    ///
    /// The transport must be initialized and available. Cancellation masks
    /// MTIE before changing the comparator.
    pub fn cancel_current(&self) -> Result<(), Error> {
        self.check_current()?;
        riscv::interrupt::machine::free(|| {
            let mode = self.mode()?;
            if mode == MODE_UNAVAILABLE {
                return Err(Error::Unavailable);
            }
            Mie::clear_bits(Mie::MACHINE_TIMER);
            match mode {
                MODE_SSTC => Stimecmp::write_time(u64::MAX).map_err(Error::Csr)?,
                MODE_PLATFORM => self
                    .platform_timer()
                    .expect("BUG: platform timer mode without a device")
                    .set_deadline(self.hart, u64::MAX),
                _ => unreachable!("BUG: invalid current hart timer mode"),
            }
            Mip::clear_bits(Mip::SUPERVISOR_TIMER);
            Ok(())
        })
    }

    /// Selects this hart's transport before trap initialization publishes Ready.
    ///
    /// An unavailable timer is an explicit mode: SBI TIME may remain absent
    /// and deadline operations return [`Error::Unavailable`].
    pub(crate) fn initialize_current_hart(&self) -> Result<(), Error> {
        self.check_current()?;
        let timer = PLATFORM_TIMER.get().ok_or(Error::NotInitialized)?.as_ref();
        let has_sstc = self.supports_sstc()?;
        if !has_sstc && timer.is_none() && REQUIRED.load(Ordering::Acquire) {
            return Err(Error::Unavailable);
        }
        Mie::clear_bits(Mie::MACHINE_TIMER);
        if let Some(device) = timer {
            // Clear a loader's platform source even when Sstc owns deadlines.
            device.set_deadline(self.hart, u64::MAX);
        }
        if has_sstc {
            Stimecmp::write_time(u64::MAX).map_err(Error::Csr)?;
            #[cfg(target_pointer_width = "64")]
            type TimerConfig = Menvcfg;
            #[cfg(target_pointer_width = "64")]
            type TimerConfigValue = EnvironmentConfig;
            #[cfg(target_pointer_width = "32")]
            type TimerConfig = MenvcfgHigh;
            #[cfg(target_pointer_width = "32")]
            type TimerConfigValue = EnvironmentConfigHigh;
            riscv::interrupt::machine::free(|| {
                let previous = TimerConfig::read().map_err(Error::Csr)?;
                TimerConfig::write(TimerConfigValue::from_bits(
                    previous.bits() | TimerConfigValue::SUPERVISOR_TIMER,
                ))
                .map_err(Error::Csr)?;
                if TimerConfig::read().map_err(Error::Csr)?.bits()
                    & TimerConfigValue::SUPERVISOR_TIMER
                    == 0
                {
                    return Err(Error::SstcDisabled);
                }
                Ok(())
            })?;
        }
        Mip::clear_bits(Mip::SUPERVISOR_TIMER);
        self.state.mode.store(
            if has_sstc {
                MODE_SSTC
            } else if timer.is_some() {
                MODE_PLATFORM
            } else {
                MODE_UNAVAILABLE
            },
            Ordering::Release,
        );
        Ok(())
    }

    /// Acknowledges MTIP and transports expiry to the supervisor when required.
    pub(crate) fn on_machine_timer(&self) -> Result<(), Error> {
        self.check_current()?;
        Mie::clear_bits(Mie::MACHINE_TIMER);
        match self.mode()? {
            MODE_PLATFORM => {
                self.platform_timer()
                    .expect("BUG: platform timer mode without a device")
                    .acknowledge(self.hart);
                Mip::set_bits(Mip::SUPERVISOR_TIMER);
            }
            MODE_SSTC => {}
            MODE_UNAVAILABLE => return Err(Error::Unavailable),
            _ => unreachable!("BUG: invalid current hart timer mode"),
        }
        Ok(())
    }

    /// Restores this hart's timer transport for a fresh next-stage entry.
    pub(crate) fn prepare_next_stage(&self) -> Result<(), Error> {
        self.check_current()?;
        match self.mode()? {
            MODE_PLATFORM => Mie::set_bits(Mie::MACHINE_TIMER),
            MODE_SSTC | MODE_UNAVAILABLE => Mie::clear_bits(Mie::MACHINE_TIMER),
            _ => unreachable!("BUG: invalid current hart timer mode"),
        }
        Ok(())
    }

    /// Returns the platform counter used for emulating trapped `time` reads.
    pub(crate) fn platform_time_source(&self) -> Option<&'static dyn TimeSource> {
        self.platform_timer().and_then(|timer| timer.time_source)
    }

    fn platform_timer(&self) -> Option<&'static PlatformTimer> {
        PLATFORM_TIMER.get().and_then(Option::as_ref)
    }
}
