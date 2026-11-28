//! Saved integer context and immutable facts for one machine trap.

use riscv_csr_spec::{
    ReadMode, WriteMode,
    standard::isa::{
        privileged::machine::Mstatus,
        values::{
            status::{Endianness, Privilege},
            trap::{CauseValue, TrapCause},
            xlen::Xlen,
        },
    },
};

// The upper status fields live in mstatush on RV32 and in mstatus on RV64.
cfg_select! {
    target_arch = "riscv32" => {
        use riscv_csr_spec::standard::isa::privileged::machine::Mstatush as UpperStatus;

        impl TrapInfo {
            fn upper_status(&self) -> UpperStatus<ReadMode> {
                self.status_high
            }

            pub(super) fn xlen(&self, privilege: Privilege) -> Option<Xlen> {
                matches!(privilege, Privilege::User | Privilege::Supervisor).then_some(Xlen::X32)
            }
        }

        impl TrapFrame {
            fn upper_status_mut(&mut self) -> &mut UpperStatus<WriteMode> {
                &mut self.status_high
            }
        }
    }
    target_arch = "riscv64" => {
        use Mstatus as UpperStatus;

        impl TrapInfo {
            fn upper_status(&self) -> UpperStatus<ReadMode> {
                self.status
            }

            pub(super) fn xlen(&self, privilege: Privilege) -> Option<Xlen> {
                match privilege {
                    Privilege::User => self.status.get(Mstatus::UXL).ok(),
                    Privilege::Supervisor => self.status.get(Mstatus::SXL).ok(),
                    Privilege::Machine => None,
                }
            }
        }

        impl TrapFrame {
            fn upper_status_mut(&mut self) -> &mut UpperStatus<WriteMode> {
                &mut self.status
            }
        }
    }
}

impl TrapFrame {
    pub(super) fn leave_virtualization(&mut self) {
        self.upper_status_mut().set(UpperStatus::MPV, false);
    }

    /// Takes the saved landing-pad expectation from `MPELP`.
    ///
    /// Clearing `MPELP` lets `MRET` enter the supervisor handler with
    /// `NO_LP_EXPECTED`.
    pub(super) fn take_previous_landing_pad(&mut self) -> bool {
        let status = self.upper_status_mut();
        let previous = status.get(UpperStatus::MPELP);
        status.set(UpperStatus::MPELP, false);
        previous
    }
}

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
    pub(super) status: Mstatus<ReadMode>,
    #[cfg(target_pointer_width = "32")]
    pub(super) status_high: UpperStatus<ReadMode>,
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
    pub(super) status: Mstatus<WriteMode>,
    #[cfg(target_pointer_width = "32")]
    pub(super) status_high: UpperStatus<WriteMode>,
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
    pub(super) fn status_bits(&self) -> u64 {
        let bits = self.status.bits() as u64;
        #[cfg(target_pointer_width = "32")]
        let bits = bits | (u64::from(self.status_high.bits()) << 32);
        bits
    }

    pub(super) fn cause(&self) -> TrapCause {
        CauseValue::from_bits(self.cause).cause(Xlen::NATIVE)
    }

    pub(super) fn origin(&self) -> Result<Privilege, usize> {
        self.status.get(Mstatus::MPP)
    }

    /// Returns the saved non-virtualized S/U context's effective register width.
    pub(super) fn lower_xlen(&self) -> Option<Xlen> {
        if self.virtualized() {
            return None;
        }
        self.xlen(self.origin().ok()?)
    }

    pub(super) fn virtualized(&self) -> bool {
        self.upper_status().get(UpperStatus::MPV)
    }

    pub(super) fn guest_address(&self) -> bool {
        self.upper_status().get(UpperStatus::GVA)
    }

    pub(super) fn supervisor_endianness(&self) -> Endianness {
        self.upper_status().get(UpperStatus::SBE)
    }
}

impl TrapFrame {
    pub(super) fn advance_pc(&mut self, length: usize, xlen: Xlen) {
        self.epc = match xlen {
            Xlen::X32 => (self.epc as u32).wrapping_add(length as u32) as i32 as usize,
            Xlen::X64 => self.epc.wrapping_add(length),
        };
    }
}
