use core::cell::UnsafeCell;

pub(crate) const RISCV_PLIC_COMPATIBLE: [&str; 1] = ["riscv,plic0"];
pub(crate) const THEAD_PLIC_COMPATIBLE: [&str; 1] = ["thead,c900-plic"];

#[doc(hidden)]
#[allow(unused)]
#[derive(Clone, Copy, Debug)]
pub enum PlicType {
    RiscvPlic,
    TheadPlic,
}

/// Priority Register
#[repr(transparent)]
pub struct PRIO(UnsafeCell<u32>);

/// Control Register
#[repr(transparent)]
pub struct CTRL(UnsafeCell<u32>);

#[repr(C)]
pub struct RiscvPlic {
    priority: [PRIO; 1024],
    reverve_1: [u8; 0x1feffc],
    control: CTRL,
}

pub struct PlicWrap {
    device: *const RiscvPlic,
    #[allow(unused)]
    pub device_number: u32,
    pub plic_type: PlicType,
}

impl PlicWrap {
    pub fn new(base_addr: usize, plic_type: PlicType, device_number: u32) -> Self {
        Self {
            device: base_addr as *const RiscvPlic,
            device_number,
            plic_type,
        }
    }

    pub fn set_priority(&self, i: usize, priority: u32) {
        unsafe {
            (*self.device).priority[i].0.get().write_volatile(priority);
        }
    }

    pub fn set_delegate(&self) {
        match self.plic_type {
            PlicType::TheadPlic => unsafe {
                (*self.device).control.0.get().write_volatile(1);
            },
            _ => {}
        }
    }
}
