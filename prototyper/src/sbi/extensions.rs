use serde_device_tree::buildin::NodeSeq;

use crate::sbi::trap_stack::ROOT_STACK;
pub struct HartExtensions([bool; Extension::COUNT]);

#[derive(Copy, Clone)]
pub enum Extension {
    SSTC = 0,
}

impl Extension {
    const COUNT: usize = 1;
    const ITER: [Self;Extension::COUNT] = [Extension::SSTC];

    pub fn to_str(&self) -> &'static str {
        match self {
            Extension::SSTC => "sstc",
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
            .map(|x| x.hart_context().extensions.0[ext.index()]).unwrap()
    }
}

pub fn init(cpus: &NodeSeq) {
    use crate::dt::Cpu;
    for cpu_iter in cpus.iter() {
        let cpu = cpu_iter.deserialize::<Cpu>();
        let hart_id = cpu.reg.iter().next().unwrap().0.start;
        let mut hart_exts = [false;Extension::COUNT];
        let isa = cpu.isa.unwrap();
        Extension::ITER.iter().for_each(|ext| {
            if isa.iter().any(|e| e == ext.to_str()) {
                hart_exts[ext.index()] = true;
            } else {
                hart_exts[ext.index()] = false;
            }
        });

        #[cfg(feature = "nemu")] 
        {
            hart_exts[Extension::SSTC.index()] = true;
        }

        unsafe {
            ROOT_STACK
                .get_mut(hart_id)
                .map(|stack| stack.hart_context().extensions = HartExtensions(hart_exts)).unwrap()
        }
    }
}
