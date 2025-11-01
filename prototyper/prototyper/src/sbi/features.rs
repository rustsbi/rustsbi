use ::riscv::register::mstatus::MPP;
use riscv::register::misa;
use seq_macro::seq;
use serde_device_tree::buildin::NodeSeq;

use crate::fail;
use crate::platform::CPU_PRIVILEGED_ENABLED;
use crate::riscv::csr::*;
use crate::riscv::current_hartid;
use crate::sbi::early_trap::{TrapInfo, csr_read_allow, csr_write_allow};
use crate::sbi::trap_stack::{hart_context, hart_context_mut};

use super::early_trap::csr_swap;

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
}

impl Extension {
    pub const COUNT: usize = 2;

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Sstc => "sstc",
            Self::Hypervisor => "h",
        }
    }

    #[inline]
    pub const fn index(&self) -> usize {
        *self as usize
    }

    pub fn iter() -> impl Iterator<Item = Self> {
        [Self::Sstc, Self::Hypervisor].into_iter()
    }
}

/// Probes if a specific extension is supported for the given hart.
#[inline]
pub fn hart_extension_probe(hart_id: usize, ext: Extension) -> bool {
    hart_context(hart_id).features.extensions[ext.index()]
}

/// Gets the privileged version for the given hart.
#[inline]
pub fn hart_privileged_version(hart_id: usize) -> PrivilegedVersion {
    hart_context(hart_id).features.privileged_version
}

/// Gets the MHPM mask for the given hart.
#[inline]
pub fn hart_mhpm_mask(hart_id: usize) -> u32 {
    hart_context(hart_id).features.mhpm_mask
}

/// Detects RISC-V extensions from the device tree for all harts.
#[cfg(not(feature = "nemu"))]
pub fn extension_detection(cpus: &NodeSeq) {
    use crate::devicetree::Cpu;

    for cpu_iter in cpus.iter() {
        let cpu_data = cpu_iter.deserialize::<Cpu>();
        let hart_id = cpu_data.reg.iter().next().unwrap().0.start;
        let mut extensions = [false; Extension::COUNT];

        for ext in Extension::iter() {
            let ext_index = ext.index();
            let ext_name = ext.as_str();

            let dt_supported = check_extension_in_device_tree(ext_name, &cpu_data);
            extensions[ext_index] = match ext {
                Extension::Hypervisor if hart_id == current_hartid() => {
                    misa::read().unwrap().has_extension('H')
                }
                _ => dt_supported,
            };
        }

        hart_context_mut(hart_id).features.extensions = extensions;
    }
}

fn check_extension_in_device_tree(ext: &str, cpu: &crate::devicetree::Cpu) -> bool {
    // Check isa-extensions first (preferred, list of strings)
    if let Some(isa_exts) = &cpu.isa_extensions {
        return isa_exts.iter().any(|e| e == ext);
    }

    // Fallback to isa (take first string, default to empty)
    cpu.isa
        .iter()
        .next()
        .and_then(|isa| isa.iter().next())
        .map(|isa| {
            isa.split('_')
                .any(|part| part == ext || (ext.len() == 1 && part.contains(ext)))
        })
        .unwrap_or(false)
}

fn privileged_version_detection() {
    let mut current_priv_ver = PrivilegedVersion::Unknown;
    {
        if has_csr!(CSR_MCOUNTEREN) {
            current_priv_ver = PrivilegedVersion::Version1_10;
            if has_csr!(CSR_MCOUNTINHIBIT) {
                current_priv_ver = PrivilegedVersion::Version1_11;
                if has_csr!(CSR_MENVCFG) {
                    current_priv_ver = PrivilegedVersion::Version1_12;
                }
            }
        }
    }
    hart_context_mut(current_hartid())
        .features
        .privileged_version = current_priv_ver;
}

fn mhpm_detection() {
    // The standard specifies that mcycle,minstret,mtime must be implemented
    let mut current_mhpm_mask: u32 = 0b111;
    let mut trap_info: TrapInfo = TrapInfo::default();

    fn check_mhpm_csr<const CSR_NUM: u16>(trap_info: *mut TrapInfo, mhpm_mask: &mut u32) {
        unsafe {
            let old_value = csr_read_allow::<CSR_NUM>(trap_info);
            if (*trap_info).mcause == usize::MAX {
                csr_write_allow::<CSR_NUM>(trap_info, 1);
                if (*trap_info).mcause == usize::MAX && csr_swap::<CSR_NUM>(old_value) == 1 {
                    (*mhpm_mask) |= 1 << (CSR_NUM - CSR_MCYCLE);
                }
            }
        }
    }

    macro_rules! m_check_mhpm_csr {
        ($csr_num:expr, $trap_info:expr, $value:expr) => {
            check_mhpm_csr::<$csr_num>($trap_info, $value)
        };
    }

    // CSR_MHPMCOUNTER3:   0xb03
    // CSR_MHPMCOUNTER31:  0xb1f
    seq!(csr_num in 0xb03..=0xb1f{
        m_check_mhpm_csr!(csr_num, &mut trap_info, &mut current_mhpm_mask);
    });

    hart_context_mut(current_hartid()).features.mhpm_mask = current_mhpm_mask;
    // TODO: at present, rustsbi prptotyper only supports 64bit.
    hart_context_mut(current_hartid()).features.mhpm_bits = 64;
}

pub fn hart_features_detection() {
    privileged_version_detection();
    mhpm_detection();
}

#[cfg(feature = "nemu")]
pub fn init(cpus: &NodeSeq) {
    for hart_id in 0..cpus.len() {
        let mut hart_exts = [false; Extension::COUNT];
        hart_exts[Extension::Sstc.index()] = true;
        hart_context(hart_id).features = HartFeatures {
            extension: hart_exts,
            privileged_version: PrivilegedVersion::Version1_12,
        }
    }
}

// Check if current cpu support target privillege.
//
// If not, go to loop trap sliently.
pub fn hart_privileged_check(mpp: MPP) {
    let hart_id = current_hartid();
    match mpp {
        MPP::Supervisor => {
            if !misa::read().unwrap().has_extension('S') {
                warn!("Hart {} not support Supervisor mode", hart_id);
                fail::stop();
            } else {
                unsafe {
                    CPU_PRIVILEGED_ENABLED[hart_id] = true;
                }
            }
        }
        MPP::User => {
            if !misa::read().unwrap().has_extension('U') {
                warn!("Hart {} not support User mode", hart_id);
                fail::stop();
            } else {
                unsafe {
                    CPU_PRIVILEGED_ENABLED[hart_id] = true;
                }
            }
        }
        _ => {}
    }
}
