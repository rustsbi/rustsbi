use riscv_csr_spec::standard::isa::values::status::Privilege;
use runtime::{
    Error, FdtNode, Result, boot,
    hart::{self, HartId, PerHart, Xlen},
    timer,
};
use spin::{Mutex, Once};

use super::pmu;

/// SBI-related features and privilege state for one hart.
#[derive(Default)]
pub struct HartFeatures {
    extensions: [bool; Extension::COUNT],
    privileged_version: PrivilegedVersion,
    supervisor_xlen: Option<Xlen>,
    privilege_checked: bool,
}

impl HartFeatures {
    pub const fn privileged_version(&self) -> PrivilegedVersion {
        self.privileged_version
    }
}

/// Per-hart feature records, published before platform discovery.
static FEATURES: Once<PerHart<Mutex<HartFeatures>>> = Once::new();

/// Creates one feature record per enabled hart on the boot hart.
///
/// The caller must run this after Runtime publishes the hart topology
/// and before platform discovery seeds secondary harts' features.
pub(crate) fn init() -> Result<()> {
    FEATURES.try_call_once(|| PerHart::new(|_| Mutex::new(HartFeatures::default())))?;
    Ok(())
}

fn features(hart: HartId) -> &'static Mutex<HartFeatures> {
    FEATURES
        .get()
        .expect("BUG: hart feature state used before initialization")
        .get(hart)
}

pub(crate) fn has_extension(hart: HartId, extension: Extension) -> bool {
    features(hart).lock().extensions[extension.index()]
}

pub(crate) fn privileged_version(hart: HartId) -> PrivilegedVersion {
    features(hart).lock().privileged_version()
}

pub(crate) fn supervisor_xlen(hart: HartId) -> Option<Xlen> {
    features(hart).lock().supervisor_xlen
}

pub(crate) fn hart_privilege_checked(hart: HartId) -> bool {
    features(hart).lock().privilege_checked
}

pub(crate) fn enabled_harts() -> impl Iterator<Item = HartId> {
    HartId::all().filter(|hart| hart_privilege_checked(*hart))
}

/// Returns one hart's privileged version and counter mask.
pub(crate) fn counter_topology(hart: HartId) -> (PrivilegedVersion, u32) {
    (privileged_version(hart), pmu::hardware_counter_mask(hart))
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

/// Parses the extensions reported for one CPU node.
pub(crate) fn parse_extensions(cpu: FdtNode<'_, '_>) -> [bool; Extension::COUNT] {
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

    extensions
}

/// Installs the extensions parsed from one enabled CPU node.
pub(crate) fn install_extensions(hart: HartId, extensions: [bool; Extension::COUNT]) {
    features(hart).lock().extensions = extensions;
}

/// Publishes hardware features and initializes PMU state for the calling hart.
///
/// # Safety
///
/// 1. Run on `hart` in M-mode while startup owns its programmable counters,
///    event selectors and inhibit bits. Probing can change all three.
/// 2. Keep Runtime's trap entry active with interrupts masked, `MPRV` clear
///    and `mscratch` zero.
pub(crate) unsafe fn detect_hart_features(hart: HartId) {
    let has_h = hart::has_hypervisor();
    let version = hart::detect_privileged_version();
    let xlen = hart::supervisor_xlen();
    // SAFETY: Startup owns the programmable counters and has installed trap recovery.
    let widths = unsafe { hart::detect_performance_counters() };
    {
        let mut record = features(hart).lock();
        record.extensions[Extension::Hypervisor.index()] = has_h;
        record.privileged_version = version;
        record.supervisor_xlen = xlen;
    }
    pmu::init_current(hart, widths);
}

/// Checks that this hart supports the requested privilege mode.
///
/// `hart` must identify the current hart.
pub fn check_next_stage_privilege(hart: HartId, next_mode: Privilege) -> Result<()> {
    if matches!(next_mode, Privilege::Supervisor | Privilege::User) {
        if !hart::supports_privilege(next_mode) {
            return Err(Error::NotSupported);
        }
        features(hart).lock().privilege_checked = true;
    }
    Ok(())
}

/// Configures the current hart's environment for supervisor handoff.
///
/// # Safety
///
/// 1. Run on `hart` in M-mode with local interrupts disabled and Runtime recovery
///    ready, while startup owns its counters and lower-mode state.
/// 2. Register platform interrupt devices before configuring lower-mode access.
/// 3. Keep lower-mode address spaces inactive. On a T-Head hart where MAEE will
///    be cleared, confirm that the implementation supports that use of `mxstatus`.
pub(crate) unsafe fn configure_hart_environment(hart: HartId) -> Result<()> {
    let svpbmt = has_extension(hart, Extension::Svpbmt);
    // Standard Sv32 and Svpbmt page tables must not use T-Head MAEE.
    if cfg!(target_pointer_width = "32") || svpbmt {
        // SAFETY: The caller owns the address-space transition and establishes
        // the board-specific meaning of MXSTATUS before this recoverable access.
        unsafe { boot::disable_thead_maee() };
    }
    let hart_priv_version = privileged_version(hart);
    if hart_priv_version >= PrivilegedVersion::Version1_11 {
        // SAFETY: Startup owns the programmable counters before S-mode entry.
        unsafe { boot::inhibit_programmable_counters() };
    }
    timer::init_hart()?;
    let cache_management = has_extension(hart, Extension::Zicbom);
    let cache_zero = has_extension(hart, Extension::Zicboz);
    // SAFETY: Startup owns the lower-mode environment and recovery is active
    // if the hart lacks an environment-configuration CSR.
    unsafe { boot::configure_supervisor_envcfg(cache_management, cache_zero, svpbmt) }?;
    if has_extension(hart, Extension::Zkr) {
        boot::enable_supervisor_seed();
    }
    // SAFETY: Platform binding selected the IPI device, and startup still owns
    // this hart's state-enable registers.
    unsafe { boot::configure_state_access(has_extension(hart, Extension::Smaia)) };
    Ok(())
}
