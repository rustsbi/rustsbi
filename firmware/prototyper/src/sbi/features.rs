use ::riscv::register::mstatus::MPP;
use core::fmt;
use runtime::FdtNode;
pub use runtime::features::PrivilegedVersion;
use runtime::features::SupervisorEnvironmentPolicy;
use runtime::{features as arch_features, pmu};

use crate::fail;
use crate::platform::mark_hart_privilege_checked;
use crate::sbi::hart_local::{with_current, with_hart};
use runtime::hart::HartId;

/// A failure while detecting or preparing the current hart for supervisor use.
#[derive(Debug)]
pub(crate) enum HartInitError {
    PrivilegedVersion(arch_features::FeatureError),
    CounterReset(pmu::CounterError),
    SupervisorEnvironment(arch_features::FeatureError),
    Timer(runtime::timer::Error),
}

impl fmt::Display for HartInitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PrivilegedVersion(error) => {
                write!(
                    formatter,
                    "privileged architecture discovery failed: {error}"
                )
            }
            Self::Timer(error) => write!(formatter, "timer discovery failed: {error}"),
            Self::CounterReset(error) => write!(formatter, "counter reset failed: {error:?}"),
            Self::SupervisorEnvironment(error) => {
                write!(formatter, "supervisor environment setup failed: {error}")
            }
        }
    }
}

#[derive(Default)]
pub struct HartFeatures {
    extensions: [bool; Extension::COUNT],
    privileged_version: PrivilegedVersion,
}

impl HartFeatures {
    pub const fn privileged_version(&self) -> PrivilegedVersion {
        self.privileged_version
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Extension {
    Sstc = 0,
    Hypervisor = 1,
    Smaia = 2,
    Svpbmt = 3,
    Zkr = 4,
    // `COUNT` and `iter()` must cover every variant used as an array index.
}

impl Extension {
    pub const COUNT: usize = 5;

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Sstc => "sstc",
            Self::Hypervisor => "h",
            Self::Smaia => "smaia",
            Self::Svpbmt => "svpbmt",
            Self::Zkr => "zkr",
        }
    }

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
        ]
        .into_iter()
    }
}

/// Returns whether a specific extension is supported for the given hart.
pub fn hart_has_extension(hart_id: usize, extension: Extension) -> bool {
    with_hart(hart_id, |local| {
        local.with_features(|features| features.extensions[extension.index()])
    })
}

/// Returns the privileged-architecture version for the given hart.
pub fn hart_privileged_version(hart_id: usize) -> PrivilegedVersion {
    with_hart(hart_id, |local| {
        local.with_features(HartFeatures::privileged_version)
    })
}

/// Records the extensions of one enabled hart during CPU discovery.
#[cfg(not(feature = "nemu"))]
pub fn detect_extensions(hart_id: usize, cpu: FdtNode<'_, '_>) {
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

    if hart_id
        == HartId::current()
            .expect("BUG: current hart exceeds Runtime capacity")
            .as_usize()
    {
        extensions[Extension::Hypervisor.index()] = environment().supports_hypervisor_mode();
    }
    with_hart(hart_id, |local| {
        local.with_features_mut(|features| features.extensions = extensions)
    });
}

/// Preserves the fixed NEMU feature profile installed by [`init`].
#[cfg(feature = "nemu")]
pub fn detect_extensions(_hart_id: usize, _cpu: FdtNode<'_, '_>) {}

fn detect_privileged_version() -> Result<(), HartInitError> {
    let privileged_version = environment()
        .probe_privileged_version()
        .map_err(HartInitError::PrivilegedVersion)?;
    with_current(|local| {
        local.with_features_mut(|features| features.privileged_version = privileged_version)
    });
    Ok(())
}

/// Detects Sstc even when it is omitted from the device tree.
fn detect_sstc() -> Result<(), HartInitError> {
    let sstc = runtime::timer::Timer::current()
        .and_then(|timer| timer.supports_sstc())
        .map_err(HartInitError::Timer)?;
    with_current(|local| {
        local.with_features_mut(|features| features.extensions[Extension::Sstc.index()] = sstc)
    });
    Ok(())
}

fn detect_mhpm_counters() {
    let counters = pmu::Pmu::current()
        .and_then(|pmu| pmu.probe())
        .expect("failed to discover current-hart counters");
    with_current(|local| local.init_pmu(counters));
}

/// Detects the current hart's privileged-architecture version, Sstc support
/// and hardware counters after device-tree discovery.
pub(crate) fn detect_hart_features() -> Result<(), HartInitError> {
    detect_privileged_version()?;
    detect_sstc()?;
    detect_mhpm_counters();
    Ok(())
}

#[cfg(feature = "nemu")]
pub fn init(cpus: FdtNode<'_, '_>) {
    let hart_count = cpus
        .children()
        .filter(|node| crate::devicetree::is_cpu_node(*node))
        .count();
    for hart_id in 0..hart_count {
        let mut hart_exts = [false; Extension::COUNT];
        hart_exts[Extension::Sstc.index()] = true;
        with_hart(hart_id, |local| {
            local.with_features_mut(|features| {
                *features = HartFeatures {
                    extensions: hart_exts,
                    privileged_version: PrivilegedVersion::Version1_12,
                }
            })
        });
    }
}

/// Checks that this hart supports the requested privilege mode.
///
/// Warns and stops the hart if it does not.
pub fn check_next_stage_privilege(next_mode: MPP) {
    let hart_id = HartId::current()
        .expect("BUG: current hart exceeds Runtime capacity")
        .as_usize();
    match next_mode {
        MPP::Supervisor => {
            if !environment().supports_supervisor_mode() {
                warn!("Hart {} does not support Supervisor mode", hart_id);
                fail::stop();
            }
            mark_hart_privilege_checked(hart_id);
        }
        MPP::User => {
            if !environment().supports_user_mode() {
                warn!("Hart {} does not support User mode", hart_id);
                fail::stop();
            }
            mark_hart_privilege_checked(hart_id);
        }
        _ => {}
    }
}

/// Resets hardware counters and applies the discovered supervisor feature policy.
///
/// Returns access failures to the boot caller before Runtime activates the final
/// trap vector.
pub(crate) fn configure_hart_environment() -> Result<(), HartInitError> {
    let imsic_ipis = crate::platform::interrupts().supervisor_aia();
    let (policy, standard_page_memory_types) = with_current(|local| {
        local.with_features(|features| {
            // C907 advertises its RV32 page-memory-type extension as Svpbmt.
            let svpbmt = features.extensions[Extension::Svpbmt.index()];
            (
                SupervisorEnvironmentPolicy {
                    page_based_memory_types: svpbmt,
                    supervisor_seed: features.extensions[Extension::Zkr.index()],
                    supervisor_aia: imsic_ipis && features.extensions[Extension::Smaia.index()],
                },
                // Standard RV32 page tables and Svpbmt require T-Head MAEE off.
                cfg!(target_pointer_width = "32") || svpbmt,
            )
        })
    });
    pmu::Pmu::current()
        .and_then(|pmu| pmu.reset())
        .map_err(HartInitError::CounterReset)?;
    if standard_page_memory_types
        && let Some(thead) = runtime::soc::thead::THead::current()
            .expect("BUG: current hart outside published topology")
    {
        thead.use_standard_page_memory_types().map_err(|error| {
            HartInitError::SupervisorEnvironment(arch_features::FeatureError::Access(error))
        })?;
    }
    environment()
        .configure(policy)
        .map_err(HartInitError::SupervisorEnvironment)
}

fn environment() -> arch_features::SupervisorEnvironment {
    arch_features::SupervisorEnvironment::current()
        .expect("BUG: current hart outside published topology")
}
