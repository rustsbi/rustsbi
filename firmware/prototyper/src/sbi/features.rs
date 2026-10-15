use riscv::register::mstatus::MPP;
use runtime::{
    FdtNode, boot,
    hart::{self, HartId},
    ipi::IpiDelivery,
    timer::Timer,
};

use crate::fail;
use crate::platform::mark_hart_privilege_checked;
use crate::riscv::csr::*;
use crate::sbi::hart_local::{with_current, with_hart};

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
            Self::Smaia => "smaia", // TODO verify with DTB standard
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

/// Returns whether a specific extension is supported for the given hart.
#[inline]
pub fn hart_has_extension(hart_id: usize, extension: Extension) -> bool {
    with_hart(hart_id, |local| {
        local.with_features(|features| features.extensions[extension.index()])
    })
}

/// Gets the privileged version for the given hart.
#[inline]
pub fn hart_privileged_version(hart_id: usize) -> PrivilegedVersion {
    with_hart(hart_id, |local| {
        local.with_features(HartFeatures::privileged_version)
    })
}

/// Gets the MHPM mask for the given hart.
#[inline]
pub fn hart_mhpm_mask(hart_id: usize) -> u32 {
    with_hart(hart_id, |local| {
        local.with_features(HartFeatures::mhpm_mask)
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
        extensions[Extension::Hypervisor.index()] = hart::has_hypervisor();
    }
    with_hart(hart_id, |local| {
        local.with_features_mut(|features| features.extensions = extensions)
    });
}

/// NEMU supplies a fixed feature profile through [`init`].
#[cfg(feature = "nemu")]
pub fn detect_extensions(_hart_id: usize, _cpu: FdtNode<'_, '_>) {}

fn detect_privileged_version() {
    let privileged_version = hart::detect_privileged_version();
    with_current(|local| {
        local.with_features_mut(|features| features.privileged_version = privileged_version)
    });
}

/// Detects Sstc even when it is omitted from the device tree.
fn detect_sstc() {
    let sstc = Timer::supports_sstc();
    with_current(|local| {
        local.with_features_mut(|features| features.extensions[Extension::Sstc.index()] = sstc)
    });
}

fn detect_mhpm_counters() {
    // SAFETY: Startup owns these counters before the PMU is published.
    let mhpm_mask = unsafe { hart::detect_performance_counters() };

    with_current(|local| {
        local.with_features_mut(|features| {
            features.mhpm_mask = mhpm_mask;
            // TODO: at present, the prototyper only supports 64-bit counters.
            features.mhpm_bits = 64;
        });
        // The PMU state snapshots the counter topology; rebuild it now that
        // the mask is known.
        local.init_pmu();
    });
}

/// Detects the current hart's privileged-architecture version, Sstc support and
/// hardware counters after device-tree discovery.
pub fn detect_hart_features() {
    detect_privileged_version();
    detect_sstc();
    detect_mhpm_counters();
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
                    mhpm_mask: 0,
                    mhpm_bits: 0,
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
            if !hart::supports_privilege(MPP::Supervisor) {
                warn!("Hart {} does not support Supervisor mode", hart_id);
                fail::stop();
            }
            mark_hart_privilege_checked(hart_id);
        }
        MPP::User => {
            if !hart::supports_privilege(MPP::User) {
                warn!("Hart {} does not support User mode", hart_id);
                fail::stop();
            }
            mark_hart_privilege_checked(hart_id);
        }
        _ => {}
    }
}

/// Configures the per-hart S-mode environment CSRs for supervisor hand-off
/// (counter inhibits and environment features).
///
/// Runtime boot owns delegation, counter access and trap-vector setup.
pub fn configure_hart_environment() {
    let hart_id = HartId::current()
        .expect("BUG: current hart exceeds Runtime capacity")
        .as_usize();
    // Standard Sv32 and Svpbmt page tables must not use T-Head MAEE.
    if cfg!(target_pointer_width = "32") || hart_has_extension(hart_id, Extension::Svpbmt) {
        // SAFETY: M-mode startup has installed Runtime entry, with mscratch zero,
        // MPRV clear, and no lower-mode address space active on this hart.
        unsafe { boot::disable_thead_maee() };
    }
    let hart_priv_version = hart_privileged_version(hart_id);
    if hart_priv_version >= PrivilegedVersion::Version1_11 {
        mcountinhibit::write_raw(!0b111usize);
    }
    // Sstc provides STCE independently of the privileged-version heuristic.
    // Use the same per-hart result as the SBI timer and Runtime interrupt path.
    if hart_has_extension(hart_id, Extension::Sstc) {
        menvcfg::set_bits(menvcfg::STCE);
    }
    if hart_priv_version >= PrivilegedVersion::Version1_12 {
        for (extension, bits) in [
            (Extension::Zicbom, menvcfg::CBIE_INVALIDATE | menvcfg::CBCFE),
            (Extension::Zicboz, menvcfg::CBZE),
        ] {
            if hart_has_extension(hart_id, extension) {
                menvcfg::set_bits(bits);
            } else {
                menvcfg::clear_bits(bits);
            }
        }
        // Follow the device tree: C907 firmware also describes its RV32
        // page-memory-type extension as Svpbmt and requires PBMTE.
        if hart_has_extension(hart_id, Extension::Svpbmt) {
            menvcfg::set_bits(menvcfg::PBMTE);
        }
        // The S-mode access to the Zkr `seed` CSR is gated under `mseccfg.SSEED`.
        if hart_has_extension(hart_id, Extension::Zkr) {
            boot::enable_supervisor_seed();
        }
        let enable_aia = matches!(
            crate::platform::ipi_delivery(),
            Some(IpiDelivery::Imsic { .. })
        ) && hart_has_extension(hart_id, Extension::Smaia);
        boot::configure_state_access(enable_aia);
    }
}
