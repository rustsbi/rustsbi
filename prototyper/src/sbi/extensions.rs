use serde_device_tree::buildin::NodeSeq;

use crate::sbi::trap_stack::ROOT_STACK;
pub struct HartExtensions([bool; Extension::COUNT]);

#[derive(Copy, Clone)]
pub enum Extension {
    Sstc = 0,
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
            .map(|x| x.hart_context().extensions.0[ext.index()])
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
                .map(|stack| stack.hart_context().extensions = HartExtensions(hart_exts))
                .unwrap()
        }
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
                .map(|stack| stack.hart_context().extensions = HartExtensions(hart_exts))
                .unwrap()
        }
    }
}
