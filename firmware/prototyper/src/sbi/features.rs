#![forbid(unsafe_code)]

use ::riscv::register::mstatus::MPP;
use riscv::register::misa;
use seq_macro::seq;
use serde_device_tree::buildin::{Node, NodeSeq};

use crate::fail;
use crate::platform::mark_hart_privilege_checked;
use crate::riscv::csr::*;
use crate::riscv::current_hartid;
use crate::sbi::early_trap::TrapInfo;
use crate::sbi::trap_stack::{hart_local, with_current, with_hart};
use runtime::node_is_enabled;

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
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PrivilegedVersion {
    Unknown = 0,
    Version1_10 = 1,
    Version1_11 = 2,
    Version1_12 = 3,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Extension {
    Sstc = 0,
    Hypervisor = 1,
    Smaia = 2,
    // Remember to increment `Extension::COUNT` while implementing new extensions.
}

impl Extension {
    pub const COUNT: usize = 3;

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Sstc => "sstc",
            Self::Hypervisor => "h",
            Self::Smaia => "smaia", // TODO verify with DTB standard
        }
    }

    #[inline]
    pub const fn index(&self) -> usize {
        *self as usize
    }

    pub fn iter() -> impl Iterator<Item = Self> {
        [Self::Sstc, Self::Hypervisor, Self::Smaia].into_iter()
    }
}

/// Returns whether a specific extension is supported for the given hart.
#[inline]
pub fn hart_has_extension(hart_id: usize, extension: Extension) -> bool {
    hart_local(hart_id).features.extensions[extension.index()]
}

/// Gets the privileged version for the given hart.
#[inline]
pub fn hart_privileged_version(hart_id: usize) -> PrivilegedVersion {
    hart_local(hart_id).features.privileged_version
}

/// Gets the MHPM mask for the given hart.
#[inline]
pub fn hart_mhpm_mask(hart_id: usize) -> u32 {
    hart_local(hart_id).features.mhpm_mask
}

/// Detects RISC-V extensions from the device tree for all harts.
#[cfg(not(feature = "nemu"))]
pub fn detect_extensions(cpus: &NodeSeq, enabled_harts: &[bool]) {
    use crate::devicetree::Cpu;

    for cpu_node in cpus.iter() {
        let node = cpu_node.deserialize::<Node>();
        if !node_is_enabled(&node) {
            continue;
        }
        let cpu = cpu_node.deserialize::<Cpu>();
        let Some(hart_id) = cpu.reg.iter().next().map(|register| register.0.start) else {
            continue;
        };
        if enabled_harts.get(hart_id) != Some(&true) {
            continue;
        }
        let mut extensions = [false; Extension::COUNT];

        for extension in Extension::iter() {
            let extension_index = extension.index();
            let extension_name = extension.as_str();

            let described_by_device_tree = device_tree_has_extension(extension_name, &cpu);
            extensions[extension_index] = match extension {
                Extension::Hypervisor if hart_id == current_hartid() => {
                    misa::read().has_extension('H')
                }
                _ => described_by_device_tree,
            };
        }

        with_hart(hart_id, |local| local.features.extensions = extensions);
    }
}

fn device_tree_has_extension(extension: &str, cpu: &crate::devicetree::Cpu) -> bool {
    // Check isa-extensions first (preferred, list of strings)
    if let Some(isa_extensions) = &cpu.isa_extensions {
        return isa_extensions.iter().any(|name| name == extension);
    }

    // Fallback to isa (take first string, default to empty)
    cpu.isa
        .iter()
        .next()
        .and_then(|isa| isa.iter().next())
        .map(|isa| {
            isa.split('_')
                .any(|part| part == extension || (extension.len() == 1 && part.contains(extension)))
        })
        .unwrap_or(false)
}

fn detect_privileged_version() {
    let mut privileged_version = PrivilegedVersion::Unknown;
    {
        if has_csr::<CSR_MCOUNTEREN>() {
            privileged_version = PrivilegedVersion::Version1_10;
            if has_csr::<CSR_MCOUNTINHIBIT>() {
                privileged_version = PrivilegedVersion::Version1_11;
                if has_csr::<CSR_MENVCFG>() {
                    privileged_version = PrivilegedVersion::Version1_12;
                }
            }
        }
    }
    with_current(|local| local.features.privileged_version = privileged_version);
}

fn detect_mhpm_counters() {
    // mcycle, minstret, and time are treated as always implemented;
    // bits 0-2 of the mask record them.
    let mut mhpm_mask: u32 = 0b111;
    let mut trap: TrapInfo = TrapInfo::default();

    macro_rules! m_probe_mhpm_csr {
        ($csr_num:expr, $trap_info:expr, $value:expr) => {
            probe_mhpm_csr::<$csr_num>($trap_info, $value)
        };
    }

    // CSR_MHPMCOUNTER3:   0xb03
    // CSR_MHPMCOUNTER31:  0xb1f
    seq!(csr_num in 0xb03..=0xb1f{
        m_probe_mhpm_csr!(csr_num, &mut trap, &mut mhpm_mask);
    });

    with_current(|local| {
        local.features.mhpm_mask = mhpm_mask;
        // TODO: at present, the prototyper only supports 64-bit counters.
        local.features.mhpm_bits = 64;
    });
}

/// Detects the current hart's privileged-architecture version and hardware
/// counters.
pub fn detect_hart_features() {
    detect_privileged_version();
    detect_mhpm_counters();
}

#[cfg(feature = "nemu")]
pub fn init(cpus: &NodeSeq) {
    for hart_id in 0..cpus.len() {
        let mut hart_exts = [false; Extension::COUNT];
        hart_exts[Extension::Sstc.index()] = true;
        hart_local(hart_id).features = HartFeatures {
            extension: hart_exts,
            privileged_version: PrivilegedVersion::Version1_12,
        }
    }
}

/// Checks that this hart supports the requested privilege mode.
///
/// Warns and stops the hart if it does not.
pub fn check_next_stage_privilege(next_mode: MPP) {
    let hart_id = current_hartid();
    match next_mode {
        MPP::Supervisor => {
            if !misa::read().has_extension('S') {
                warn!("Hart {} does not support Supervisor mode", hart_id);
                fail::stop();
            }
            mark_hart_privilege_checked(hart_id);
        }
        MPP::User => {
            if !misa::read().has_extension('U') {
                warn!("Hart {} does not support User mode", hart_id);
                fail::stop();
            }
            mark_hart_privilege_checked(hart_id);
        }
        _ => {}
    }
}

/// Returns whether the `mstateen0` CSR is implemented (trap-tolerant probe).
#[inline(always)]
fn has_mstateen0() -> bool {
    has_csr::<CSR_MSTATEEN0>()
}

/// Configures per-hart delegation and trap CSRs for supervisor hand-off.
pub fn configure_delegation_and_trap() {
    configure_delegation();

    let hart_priv_version = hart_privileged_version(current_hartid());
    if hart_priv_version >= PrivilegedVersion::Version1_11 {
        mcountinhibit::write_raw(!0b111usize);
    }
    if hart_priv_version >= PrivilegedVersion::Version1_12 {
        if hart_has_extension(current_hartid(), Extension::Sstc) {
            menvcfg::set_bits(
                menvcfg::STCE | menvcfg::CBIE_INVALIDATE | menvcfg::CBCFE | menvcfg::CBZE,
            );
        } else {
            menvcfg::set_bits(menvcfg::CBIE_INVALIDATE | menvcfg::CBCFE | menvcfg::CBZE);
        }
        if crate::sbi::ipi::uses_imsic()
            && hart_has_extension(current_hartid(), Extension::Smaia)
            && has_mstateen0()
        {
            mstateen::enable_smode_aia();
        }
    }
    install_trap_vector();
}
