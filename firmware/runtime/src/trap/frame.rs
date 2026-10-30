//! Saved integer context and immutable facts for one machine trap.

use csr::{Read, Write, machine};

macro_rules! general_regs {
    ($($name:ident = $id:literal),* $(,)?) => {
        #[repr(C)]
        #[derive(Debug, Default)]
        pub(super) struct GeneralRegs {
            $(pub(super) $name: usize,)*
        }

        // Entry addresses integer registers by architectural register number.
        const _: () = {
            $(assert!(core::mem::offset_of!(GeneralRegs, $name)
                == ($id - 1) * size_of::<usize>());)*
            assert!(size_of::<GeneralRegs>() == 31 * size_of::<usize>());
        };

        impl GeneralRegs {
            pub(super) fn read(&self, id: u8) -> usize {
                match id {
                    0 => 0,
                    $($id => self.$name,)*
                    _ => unreachable!("invalid integer register"),
                }
            }

            pub(super) fn write(&mut self, id: u8, value: usize) {
                match id {
                    0 => {}
                    $($id => self.$name = value,)*
                    _ => unreachable!("invalid integer register"),
                }
            }
        }
    };
}

general_regs! {
    ra = 1, sp = 2, gp = 3, tp = 4,
    t0 = 5, t1 = 6, t2 = 7, s0 = 8, s1 = 9,
    a0 = 10, a1 = 11, a2 = 12, a3 = 13,
    a4 = 14, a5 = 15, a6 = 16, a7 = 17,
    s2 = 18, s3 = 19, s4 = 20, s5 = 21, s6 = 22,
    s7 = 23, s8 = 24, s9 = 25, s10 = 26, s11 = 27,
    t3 = 28, t4 = 29, t5 = 30, t6 = 31,
}

/// Original machine-trap facts, unchanged by nested recovery.
#[repr(C)]
#[derive(Debug)]
pub(super) struct TrapInfo {
    pub(super) epc: usize,
    pub(super) status: machine::Status<Read>,
    #[cfg(target_pointer_width = "32")]
    pub(super) status_high: machine::StatusHigh<Read>,
    #[cfg(target_pointer_width = "64")]
    pub(super) status_high: usize,
    /// Whether entry saved the H extension's trap metadata.
    pub(super) has_hypervisor: bool,
    pub(super) cause: usize,
    pub(super) tval: usize,
    pub(super) tval2: usize,
    pub(super) instruction: usize,
}

/// Integer registers and machine state restored on trap return.
// Entry must initialize every field before Rust borrows this frame.
#[repr(C)]
#[derive(Debug)]
pub(super) struct TrapFrame {
    pub(super) general: GeneralRegs,
    pub(super) epc: usize,
    pub(super) status: machine::Status<Write>,
    #[cfg(target_pointer_width = "32")]
    pub(super) status_high: machine::StatusHigh<Write>,
    #[cfg(target_pointer_width = "64")]
    pub(super) status_high: usize,
}

/// Return context and original facts for one machine trap.
// Entry initializes both parts before dispatch borrows them separately.
#[repr(C)]
#[derive(Debug)]
pub(super) struct SavedTrap {
    pub(super) frame: TrapFrame,
    pub(super) info: TrapInfo,
    /// Clean Runtime stack top before entry, or zero for a nested Runtime trap.
    pub(super) runtime_stack: usize,
}

impl TrapInfo {
    pub(super) fn origin(&self) -> machine::Privilege {
        self.status.read_field::<machine::MPP>()
    }

    pub(super) fn virtualized(&self) -> bool {
        #[cfg(target_pointer_width = "32")]
        {
            self.status_high.read_field::<machine::MPV>() == machine::Virtualization::On
        }
        #[cfg(target_pointer_width = "64")]
        {
            self.status.read_field::<machine::MPV>() == machine::Virtualization::On
        }
    }

    pub(super) fn guest_address(&self) -> bool {
        #[cfg(target_pointer_width = "32")]
        {
            self.status_high.read_field::<machine::GVA>() == machine::GuestAddress::GuestVirtual
        }
        #[cfg(target_pointer_width = "64")]
        {
            self.status.read_field::<machine::GVA>() == machine::GuestAddress::GuestVirtual
        }
    }
}

impl TrapFrame {
    pub(super) fn leave_virtualization(&mut self) {
        #[cfg(target_pointer_width = "32")]
        {
            self.status_high
                .write_field::<machine::MPV>(machine::Virtualization::Off);
        }
        #[cfg(target_pointer_width = "64")]
        {
            self.status
                .write_field::<machine::MPV>(machine::Virtualization::Off);
        }
    }
}
