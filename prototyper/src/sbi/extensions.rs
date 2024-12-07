use serde_device_tree::buildin::NodeSeq;

use crate::riscv_spec::current_hartid;
use crate::sbi::trap::expected_trap;
use crate::sbi::trap_stack::ROOT_STACK;

pub struct HartFeatures {
    extension: [bool; Extension::COUNT],
    privileged_version: PrivilegedVersion,
}

#[derive(Copy, Clone)]
pub enum Extension {
    Sstc = 0,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
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

pub fn hart_extension_probe(hart_id: usize, ext: Extension) -> bool {
    unsafe {
        ROOT_STACK
            .get_mut(hart_id)
            .map(|x| x.hart_context().features.extension[ext.index()])
            .unwrap()
    }
}

pub fn hart_privileged_version(hart_id: usize) -> PrivilegedVersion {
    unsafe {
        ROOT_STACK
            .get_mut(hart_id)
            .map(|x| x.hart_context().features.privileged_version)
            .unwrap()
    }
}

#[cfg(not(feature = "nemu"))]
pub fn init(cpus: &NodeSeq) {
    use crate::dt::Cpu;
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
                hart_exts[ext.index()] = isa.find(ext.as_str()).is_some();
            })
        }

        unsafe {
            ROOT_STACK
                .get_mut(hart_id)
                .map(|stack| stack.hart_context().features.extension = hart_exts)
                .unwrap()
        }
    }
}
pub fn privileged_version_detection() {
    let mut current_priv_ver = PrivilegedVersion::Unknown;
    {
        const CSR_MCOUNTEREN: u64 = 0x306;
        const CSR_MCOUNTINHIBIT: u64 = 0x320;
        const CSR_MENVCFG: u64 = 0x30a;

        if csr_test!(CSR_MCOUNTEREN) {
            current_priv_ver = PrivilegedVersion::Version1_10;
            if csr_test!(CSR_MCOUNTINHIBIT) {
                current_priv_ver = PrivilegedVersion::Version1_11;
                if csr_test!(CSR_MENVCFG) {
                    current_priv_ver = PrivilegedVersion::Version1_12;
                }
            }
        }
    }
    unsafe {
        ROOT_STACK
            .get_mut(current_hartid())
            .map(|stack| stack.hart_context().features.privileged_version = current_priv_ver)
            .unwrap()
    }
}

#[cfg(feature = "nemu")]
pub fn init(cpus: &NodeSeq) {
    for hart_id in 0..cpus.len() {
        let mut hart_exts = [false; Extension::COUNT];
        hart_exts[Extension::Sstc.index()] = true;
        unsafe {
            ROOT_STACK
                .get_mut(hart_id)
                .map(|stack| stack.hart_context().extensions = HartFeatures(hart_exts))
                .unwrap()
        }
    }
}
