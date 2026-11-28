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
struct HartFeatures {
    extensions: IsaExtensions,
    hypervisor: bool,
    privileged_version: PrivilegedVersion,
    supervisor_xlen: Option<Xlen>,
    privilege_checked: bool,
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

pub(super) fn has_hypervisor(hart: HartId) -> bool {
    features(hart).lock().hypervisor
}

pub(crate) fn privileged_version(hart: HartId) -> PrivilegedVersion {
    features(hart).lock().privileged_version
}

pub(crate) fn supervisor_xlen(hart: HartId) -> Option<Xlen> {
    features(hart).lock().supervisor_xlen
}

pub(super) fn instruction_alignment(hart: HartId) -> usize {
    if cfg!(target_feature = "c") || features(hart).lock().extensions.compressed {
        2
    } else {
        4
    }
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

#[derive(Clone, Copy, Default)]
struct IsaExtensions {
    envcfg: boot::SupervisorEnvConfig,
    compressed: bool,
    smaia: bool,
    zkr: bool,
}

impl IsaExtensions {
    fn insert(&mut self, name: &str) {
        match name {
            "c" | "zca" => self.compressed = true,
            "smaia" => self.smaia = true,
            "svade" => self.envcfg.svade = true,
            "svadu" => self.envcfg.svadu = true,
            "svpbmt" => self.envcfg.svpbmt = true,
            "zicbom" => self.envcfg.zicbom = true,
            "zicboz" => self.envcfg.zicboz = true,
            "zkr" => self.zkr = true,
            _ => {}
        }
    }

    // A multi-letter extension can follow the base letters without a separator.
    // Its optional version is a major number or a major/minor pair such as 1p0.
    fn insert_legacy(&mut self, component: &str) {
        let start = component.find(['s', 'x', 'z']).unwrap_or(component.len());
        let (letters, name) = component.split_at(start);
        self.compressed |= letters.contains('c');
        let digit = |c: char| c.is_ascii_digit();
        let stem = name.trim_end_matches(digit);
        if stem.len() < name.len()
            && let Some(major) = stem.strip_suffix('p')
            && major.ends_with(digit)
        {
            self.insert(major.trim_end_matches(digit));
        } else {
            self.insert(stem);
        }
    }
}

/// Records the ISA extensions used by firmware from this hart's CPU node.
pub(crate) fn install_extensions(hart: HartId, cpu: FdtNode<'_, '_>) {
    let mut extensions = IsaExtensions::default();
    if let Some(property) = cpu.property("riscv,isa-extensions") {
        for name in property
            .value
            .split_inclusive(|byte| *byte == 0)
            .map_while(|bytes| core::str::from_utf8(bytes.strip_suffix(&[0])?).ok())
        {
            extensions.insert(name);
        }
    } else if let Some(isa) = cpu
        .property("riscv,isa")
        .and_then(|property| property.as_str())
        .and_then(|isa| {
            isa.strip_prefix("rv32")
                .or_else(|| isa.strip_prefix("rv64"))
        })
    {
        for component in isa.split('_') {
            extensions.insert_legacy(component);
        }
    }
    features(hart).lock().extensions = extensions;
}

/// Publishes hardware features and initializes PMU state for the calling hart.
///
/// # Safety
///
/// 1. Run on `hart` in M-mode while startup owns its counters and inhibit bits.
/// 2. Use the hart's Runtime stack and trap entry with local interrupts masked,
///    `MPRV` clear and `mscratch` zero.
pub(crate) unsafe fn detect_hart_features(hart: HartId) -> Result<()> {
    let has_h = hart::has_hypervisor();
    let version = hart::detect_privileged_version();
    let xlen = hart::supervisor_xlen();
    // SAFETY: Startup owns local counter state and supplies trap recovery.
    unsafe { pmu::init_current(hart)? };
    {
        let mut record = features(hart).lock();
        record.hypervisor = has_h;
        record.privileged_version = version;
        record.supervisor_xlen = xlen;
    }
    Ok(())
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
/// 2. Register the platform timer and interrupt devices. The supervisor must
///    be authorized to access the state enabled by the advertised extensions.
/// 3. Keep lower-mode address spaces inactive. On a T-Head hart where MAEE will
///    be cleared, confirm that the implementation supports that use of `mxstatus`.
pub(crate) unsafe fn configure_hart_environment(hart: HartId) -> Result<()> {
    let extensions = features(hart).lock().extensions;
    // Standard Sv32 and Svpbmt page tables must not use T-Head MAEE.
    if cfg!(target_pointer_width = "32") || extensions.envcfg.svpbmt {
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
    // SAFETY: Startup owns the lower-mode environment and recovery is active
    // if the hart lacks an environment-configuration CSR.
    unsafe { boot::configure_supervisor_envcfg(extensions.envcfg) }?;
    if extensions.zkr {
        boot::enable_supervisor_seed();
    }
    // SAFETY: Platform binding selected the IPI device, and startup still owns
    // this hart's state-enable registers.
    unsafe { boot::configure_state_access(extensions.smaia) };
    Ok(())
}

pub(super) fn supervisor_envcfg(hart: HartId) -> boot::SupervisorEnvConfig {
    features(hart).lock().extensions.envcfg
}
