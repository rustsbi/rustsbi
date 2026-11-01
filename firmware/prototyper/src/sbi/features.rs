use csr::machine::TargetPrivilege as MPP;
use runtime::{
    FdtNode, boot,
    hart::{self, HartId, PerHart},
    ipi::IpiDelivery,
    timer::Timer,
};
use spin::{Mutex, Once};

use crate::fail;
use crate::platform::mark_hart_privilege_checked;
use csr::{
    machine::{self, CBCFE, CBIE, CBZE, CacheBlockInvalidateMode, FeatureState},
    values::counter::InhibitedCounters,
};

#[derive(Default)]
pub struct HartFeatures {
    extensions: [bool; Extension::COUNT],
    privileged_version: PrivilegedVersion,
    mhpm_mask: u32,
    mhpm_bits: u32,
}

impl HartFeatures {
    pub const fn privileged_version(&self) -> PrivilegedVersion {
        self.privileged_version
    }

    pub const fn mhpm_mask(&self) -> u32 {
        self.mhpm_mask
    }
}

/// Per-hart feature records, published before platform discovery.
static FEATURES: Once<PerHart<Mutex<HartFeatures>>> = Once::new();

/// Creates one feature record per enabled hart on the boot hart.
///
/// The caller must run this after Runtime publishes the hart topology and
/// before platform discovery seeds secondary harts' features.
pub(crate) fn init() {
    FEATURES.call_once(|| {
        PerHart::new(|_| Mutex::new(HartFeatures::default()))
            .expect("failed to initialize per-hart feature records")
    });
}

fn features(hart: HartId) -> &'static Mutex<HartFeatures> {
    FEATURES
        .get()
        .expect("BUG: hart feature state used before initialization")
        .get(hart)
}

fn current_features() -> &'static Mutex<HartFeatures> {
    features(HartId::current().expect("BUG: current hart is not in the boot topology"))
}

pub(crate) fn has_extension(hart: HartId, extension: Extension) -> bool {
    features(hart).lock().extensions[extension.index()]
}

pub(crate) fn privileged_version(hart: HartId) -> PrivilegedVersion {
    features(hart).lock().privileged_version()
}

pub(crate) fn mhpm_mask(hart: HartId) -> u32 {
    features(hart).lock().mhpm_mask()
}

/// Returns one hart's privileged version and counter mask.
pub(crate) fn counter_topology(hart: HartId) -> (PrivilegedVersion, u32) {
    let features = features(hart).lock();
    (features.privileged_version(), features.mhpm_mask())
}

pub use runtime::hart::PrivilegedVersion;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Extension {
    Sstc = 0,
    Hypervisor = 1,
    Smaia = 2,
    Svpbmt = 3,
    Zkr = 4,
    Zicbom = 5,
    Zicboz = 6,
    // Remember to increment `Extension::COUNT` while implementing new extensions.
}

impl Extension {
    pub const COUNT: usize = 7;

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Sstc => "sstc",
            Self::Hypervisor => "h",
            Self::Smaia => "smaia", // TODO: Verify with the device-tree specification.
            Self::Svpbmt => "svpbmt",
            Self::Zkr => "zkr",
            Self::Zicbom => "zicbom",
            Self::Zicboz => "zicboz",
        }
    }

    #[inline]
    pub const fn index(&self) -> usize {
        *self as usize
    }

    pub fn iter() -> impl Iterator<Item = Self> {
        [
            Self::Sstc,
            Self::Hypervisor,
            Self::Smaia,
            Self::Svpbmt,
            Self::Zkr,
            Self::Zicbom,
            Self::Zicboz,
        ]
        .into_iter()
    }
}

/// Records the extensions of one enabled hart during CPU discovery.
pub fn detect_extensions(hart: HartId, cpu: FdtNode<'_, '_>) {
    let mut extensions = [false; Extension::COUNT];
    if let Some(property) = cpu.property("riscv,isa-extensions") {
        for name in property.value.split(|byte| *byte == 0) {
            for extension in Extension::iter() {
                extensions[extension.index()] |= name == extension.as_str().as_bytes();
            }
        }
    } else if let Some(isa) = cpu
        .property("riscv,isa")
        .and_then(|property| property.as_str())
    {
        for part in isa.split('_') {
            for extension in Extension::iter() {
                let name = extension.as_str();
                extensions[extension.index()] |=
                    part == name || (name.len() == 1 && part.contains(name));
            }
        }
    }

    if hart == HartId::current().expect("BUG: current hart exceeds Runtime capacity") {
        extensions[Extension::Hypervisor.index()] = hart::has_hypervisor();
    }
    features(hart).lock().extensions = extensions;
}

fn detect_privileged_version() {
    let privileged_version = hart::detect_privileged_version();
    current_features().lock().privileged_version = privileged_version;
}

/// Detects Sstc even when it is omitted from the device tree.
fn detect_sstc() {
    let sstc = Timer::supports_sstc();
    current_features().lock().extensions[Extension::Sstc.index()] = sstc;
}

fn detect_mhpm_counters() {
    // SAFETY:
    // 1. This hart is in M-mode with a valid Runtime stack and trap entry;
    //    `mscratch` is zero and `MPRV` is clear.
    // 2. This hart exclusively owns its counters during startup and permits
    //    the probe to modify them before enabling SBI services.
    let mhpm_mask = unsafe { hart::detect_performance_counters() };

    {
        let mut features = current_features().lock();
        features.mhpm_mask = mhpm_mask;
        // TODO: at present, the prototyper only supports 64-bit counters.
        features.mhpm_bits = 64;
    }
    // The PMU state snapshots the counter topology; rebuild it now that
    // the mask is known.
    crate::sbi::pmu::init_current(mhpm_mask);
}

/// Detects the current hart's hardware features after device-tree discovery.
pub fn detect_hart_features() {
    detect_privileged_version();
    detect_sstc();
    detect_mhpm_counters();
}

/// Checks that this hart supports the requested privilege mode.
///
/// Reports an error and stops the hart if it does not.
pub fn check_next_stage_privilege(next_mode: MPP) {
    let hart_id = HartId::current()
        .expect("BUG: current hart exceeds Runtime capacity")
        .as_usize();
    match next_mode {
        MPP::Supervisor => {
            if !hart::supports_privilege(MPP::Supervisor) {
                error!("Hart {} does not support Supervisor mode", hart_id);
                fail::stop();
            }
            mark_hart_privilege_checked(hart_id);
        }
        MPP::User => {
            if !hart::supports_privilege(MPP::User) {
                error!("Hart {} does not support User mode", hart_id);
                fail::stop();
            }
            mark_hart_privilege_checked(hart_id);
        }
        _ => {}
    }
}

/// Configures the current hart's environment for supervisor handoff.
///
/// Runtime boot owns delegation, counter access and trap-vector setup.
pub fn configure_hart_environment() {
    let hart = HartId::current().expect("BUG: current hart exceeds Runtime capacity");
    // Standard Sv32 and Svpbmt page tables must not use T-Head MAEE.
    if cfg!(target_pointer_width = "32") || has_extension(hart, Extension::Svpbmt) {
        // SAFETY:
        // 1. M-mode startup has installed Runtime trap entry, set `mscratch` to
        //    zero, and cleared `MPRV`.
        // 2. No lower-mode address space is active on this hart during startup.
        unsafe { boot::disable_thead_maee() };
    }
    let hart_priv_version = privileged_version(hart);
    if hart_priv_version >= PrivilegedVersion::Version1_11 {
        // SAFETY: Startup owns this hart's counter initialization. The PMU
        // does not coordinate cycle-counter control shared with other harts.
        unsafe { machine::CounterInhibit::write(InhibitedCounters::programmable()) };
    }
    // Sstc provides STCE independently of the privileged-version heuristic.
    // Use the same per-hart result as the SBI timer and Runtime interrupt path.
    if has_extension(hart, Extension::Sstc) {
        #[cfg(target_pointer_width = "64")]
        use csr::machine::{EnvironmentConfig as TimerConfig, STCE as TimerEnable};
        #[cfg(target_pointer_width = "32")]
        use csr::machine::{EnvironmentConfigHigh as TimerConfig, STCEH as TimerEnable};
        // SAFETY: Startup configures this hart before lower-mode entry.
        unsafe { TimerConfig::modify(|_, value| value.set::<TimerEnable>(FeatureState::Enabled)) };
    }
    if hart_priv_version >= PrivilegedVersion::Version1_12 {
        let cache_management = has_extension(hart, Extension::Zicbom);
        let cache_zero = has_extension(hart, Extension::Zicboz);
        // SAFETY: Startup owns this hart's environment configuration.
        unsafe {
            machine::EnvironmentConfig::modify(|_, value| {
                value.set::<CBIE>(if cache_management {
                    CacheBlockInvalidateMode::Invalidate
                } else {
                    CacheBlockInvalidateMode::Disabled
                });
                value.set::<CBCFE>(FeatureState::from(cache_management));
                value.set::<CBZE>(FeatureState::from(cache_zero));
            });
        }
        // Follow the device tree: C907 firmware also describes its RV32
        // page-memory-type extension as Svpbmt and requires PBMTE.
        if has_extension(hart, Extension::Svpbmt) {
            #[cfg(target_pointer_width = "64")]
            use csr::machine::{EnvironmentConfig as MemoryConfig, PBMTE as MemoryTypeEnable};
            #[cfg(target_pointer_width = "32")]
            use csr::machine::{EnvironmentConfigHigh as MemoryConfig, PBMTEH as MemoryTypeEnable};
            // SAFETY: Startup configures this hart before lower-mode entry.
            unsafe {
                MemoryConfig::modify(|_, value| {
                    value.set::<MemoryTypeEnable>(FeatureState::Enabled)
                })
            };
        }
        // The S-mode access to the Zkr `seed` CSR is gated under `mseccfg.SSEED`.
        if has_extension(hart, Extension::Zkr) {
            boot::enable_supervisor_seed();
        }
        let enable_aia = matches!(
            crate::platform::ipi_delivery(),
            Some(IpiDelivery::Imsic { .. })
        ) && has_extension(hart, Extension::Smaia);
        boot::configure_state_access(enable_aia);
    }
}
