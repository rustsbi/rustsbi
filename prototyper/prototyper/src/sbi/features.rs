use seq_macro::seq;
use serde_device_tree::buildin::NodeSeq;

use crate::riscv::csr::*;
use crate::riscv::current_hartid;
use crate::sbi::early_trap::{TrapInfo, csr_read_allow, csr_write_allow};
use crate::sbi::trap_stack::{hart_context, hart_context_mut};

use super::early_trap::csr_swap;

pub struct HartFeatures {
    extension: [bool; Extension::COUNT],
    privileged_version: PrivilegedVersion,
    mhpm_mask: u32,
    mhpm_bits: u32,
}

#[derive(Copy, Clone)]
pub enum Extension {
    Sstc = 0,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PrivilegedVersion {
    Unknown = 0,
    Version1_10 = 1,
    Version1_11 = 2,
    Version1_12 = 3,
}

impl Extension {
    const COUNT: usize = 1;
    const ITER: [Self; Extension::COUNT] = [Extension::Sstc];

    pub fn as_str(&self) -> &'static str {
        match self {
            Extension::Sstc => "sstc",
        }
    }

    #[inline]
    pub fn index(&self) -> usize {
        *self as usize
    }
}

/// access hart feature
pub fn hart_extension_probe(hart_id: usize, ext: Extension) -> bool {
    hart_context(hart_id).features.extension[ext.index()]
}

pub fn hart_privileged_version(hart_id: usize) -> PrivilegedVersion {
    hart_context(hart_id).features.privileged_version
}

pub fn hart_mhpm_mask(hart_id: usize) -> u32 {
    hart_context(hart_id).features.mhpm_mask
}

/// Hart features detection
#[cfg(not(feature = "nemu"))]
pub fn extension_detection(cpus: &NodeSeq) {
    use crate::devicetree::Cpu;
    for cpu_iter in cpus.iter() {
        let cpu = cpu_iter.deserialize::<Cpu>();
        let hart_id = cpu.reg.iter().next().unwrap().0.start;
        let mut hart_exts = [false; Extension::COUNT];
        if cpu.isa_extensions.is_some() {
            let isa = cpu.isa_extensions.unwrap();
            Extension::ITER.iter().for_each(|ext| {
                hart_exts[ext.index()] = isa.iter().any(|e| e == ext.as_str());
            });
        } else if cpu.isa.is_some() {
            let isa_iter = cpu.isa.unwrap();
            let isa = isa_iter.iter().next().unwrap_or_default();
            Extension::ITER.iter().for_each(|ext| {
                hart_exts[ext.index()] = isa.contains(ext.as_str());
            })
        }
        hart_context_mut(hart_id).features.extension = hart_exts;
    }
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
    hart_context_mut(current_hartid()).features.privileged_version = current_priv_ver;
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
