//! Machine trap entry and return with integer-register preservation.
use csr::machine;

use super::frame::SavedTrap;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
use super::frame::{TrapFrame, TrapInfo};

pub(crate) const FRAME_BYTES: usize = core::mem::size_of::<SavedTrap>().next_multiple_of(16);

#[cfg(target_arch = "riscv32")]
const STATUS_HIGH_NUMBER: u16 = machine::StatusHigh::NUMBER;
#[cfg(target_arch = "riscv64")]
const STATUS_HIGH_NUMBER: u16 = machine::Status::NUMBER; // Unused assembly branch.

#[cfg(target_arch = "riscv32")]
macro_rules! store {
    ($reg:literal, $offset:literal, $base:literal) => {
        concat!("sw ", $reg, ", 4*(", $offset, ")(", $base, ")")
    };
}
#[cfg(target_arch = "riscv64")]
macro_rules! store {
    ($reg:literal, $offset:literal, $base:literal) => {
        concat!("sd ", $reg, ", 8*(", $offset, ")(", $base, ")")
    };
}
#[cfg(target_arch = "riscv32")]
macro_rules! load {
    ($reg:literal, $offset:literal, $base:literal) => {
        concat!("lw ", $reg, ", 4*(", $offset, ")(", $base, ")")
    };
}
#[cfg(target_arch = "riscv64")]
macro_rules! load {
    ($reg:literal, $offset:literal, $base:literal) => {
        concat!("ld ", $reg, ", 8*(", $offset, ")(", $base, ")")
    };
}

// Share the layout and restore sequence across optional-CSR variants.
// Boot selects a variant only after establishing its CSR availability.
macro_rules! trap_vector {
    ($name:ident, $high:literal, $hypervisor:literal) => {
        /// Handles a hardware machine trap using Runtime's stack protocol.
        ///
        /// # Safety
        ///
        /// Enter only through a hardware trap on this hart.
        /// In Runtime, `sp` must address a valid Runtime stack and `mscratch` must be zero.
        /// In a payload of any privilege, `mscratch` must hold this hart's clean Runtime stack top.
        /// Every optional CSR used by this entry must exist on the hart.
        #[cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), unsafe(naked))]
        pub(crate) unsafe extern "C" fn $name() {
            #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
            core::arch::naked_asm!(
                ".balign 4",
                // mscratch is zero during Runtime execution and a clean
                // Runtime stack top while running a payload of any privilege.
                "csrrw sp, {scratch}, sp",
                "bnez sp, 2f",
                "csrr sp, {scratch}",
                "addi sp, sp, -{frame_size}",
                store!( "t0", "4", "sp"),
                "li t0, 0",
                "j 3f",
                "2:",
                "addi sp, sp, -{frame_size}",
                store!( "t0", "4", "sp"),
                "addi t0, sp, {frame_size}",
                "3:",
                store!("t0", "{runtime_stack}", "sp"),
                store!( "ra", "0", "sp"),
                store!( "gp", "2", "sp"),
                store!( "tp", "3", "sp"),
                store!( "t1", "5", "sp"),
                store!( "t2", "6", "sp"),
                store!( "s0", "7", "sp"),
                store!( "s1", "8", "sp"),
                store!( "a0", "9", "sp"),
                store!( "a1", "10", "sp"),
                store!( "a2", "11", "sp"),
                store!( "a3", "12", "sp"),
                store!( "a4", "13", "sp"),
                store!( "a5", "14", "sp"),
                store!( "a6", "15", "sp"),
                store!( "a7", "16", "sp"),
                store!( "s2", "17", "sp"),
                store!( "s3", "18", "sp"),
                store!( "s4", "19", "sp"),
                store!( "s5", "20", "sp"),
                store!( "s6", "21", "sp"),
                store!( "s7", "22", "sp"),
                store!( "s8", "23", "sp"),
                store!( "s9", "24", "sp"),
                store!( "s10", "25", "sp"),
                store!( "s11", "26", "sp"),
                store!( "t3", "27", "sp"),
                store!( "t4", "28", "sp"),
                store!( "t5", "29", "sp"),
                store!( "t6", "30", "sp"),
                "csrr t0, {scratch}",
                store!("t0", "1", "sp"),
                "csrr t0, {epc_csr}",
                store!("t0", "{epc}", "sp"),
                store!("t0", "{original_epc}", "sp"),
                "csrr t0, {status_csr}",
                store!("t0", "{status}", "sp"),
                store!("t0", "{original_status}", "sp"),
                "csrr t0, {cause_csr}",
                store!("t0", "{cause}", "sp"),
                "csrr t0, {tval_csr}",
                store!("t0", "{tval}", "sp"),
                "li t0, 0",
                ".if {high}",
                "csrr t0, {status_high_csr}",
                ".endif",
                store!("t0", "{status_high}", "sp"),
                store!("t0", "{original_status_high}", "sp"),
                "li t0, 0",
                "li t1, 0",
                ".if {hypervisor}",
                "csrr t0, {tval2_csr}",
                "csrr t1, {instruction_csr}",
                ".endif",
                store!("t0", "{tval2}", "sp"),
                store!("t1", "{instruction}", "sp"),
                "li t0, {hypervisor}",
                store!("t0", "{hypervisor_offset}", "sp"),
                // Clear MPRV before Rust runs so nested probes use M-mode accesses.
                "li t0, {clear_status}",
                "csrc {status_csr}, t0",
                ".if {high}",
                "li t0, 1 << 10",
                "csrc {status_high_csr}, t0",
                ".endif",
                "csrw {scratch}, zero",
                "mv a0, sp",
                "call {dispatch}",
                // Restore return state from the frame; nested traps may change live CSRs.
                load!("t0", "{epc}", "sp"),
                "csrw {epc_csr}, t0",
                ".if {high}",
                load!("t0", "{status_high}", "sp"),
                "csrw {status_high_csr}, t0",
                ".endif",
                load!("t0", "{status}", "sp"),
                // A return to an M-mode payload republishes its Runtime stack;
                // a nested Runtime return keeps mscratch zero.
                load!("t1", "{runtime_stack}", "sp"),
                "li t2, {mpp_mask}",
                "and t3, t0, t2",
                "beq t2, t3, 4f",
                // mret clears MPRV on a lower-mode return. Clear it before
                // reloading the frame so those loads remain M-mode accesses.
                "li t1, 1 << 17",
                "not t1, t1",
                "and t0, t0, t1",
                "addi t1, sp, {frame_size}",
                "4:",
                "csrw {scratch}, t1",
                "csrw {status_csr}, t0",
                load!( "ra", "0", "sp"),
                load!( "gp", "2", "sp"),
                load!( "tp", "3", "sp"),
                load!( "t0", "4", "sp"),
                load!( "t1", "5", "sp"),
                load!( "t2", "6", "sp"),
                load!( "s0", "7", "sp"),
                load!( "s1", "8", "sp"),
                load!( "a0", "9", "sp"),
                load!( "a1", "10", "sp"),
                load!( "a2", "11", "sp"),
                load!( "a3", "12", "sp"),
                load!( "a4", "13", "sp"),
                load!( "a5", "14", "sp"),
                load!( "a6", "15", "sp"),
                load!( "a7", "16", "sp"),
                load!( "s2", "17", "sp"),
                load!( "s3", "18", "sp"),
                load!( "s4", "19", "sp"),
                load!( "s5", "20", "sp"),
                load!( "s6", "21", "sp"),
                load!( "s7", "22", "sp"),
                load!( "s8", "23", "sp"),
                load!( "s9", "24", "sp"),
                load!( "s10", "25", "sp"),
                load!( "s11", "26", "sp"),
                load!( "t3", "27", "sp"),
                load!( "t4", "28", "sp"),
                load!( "t5", "29", "sp"),
                load!( "t6", "30", "sp"),
                load!("sp", "1", "sp"),
                "mret",
                frame_size = const FRAME_BYTES,
                epc = const (core::mem::offset_of!(SavedTrap, frame) + core::mem::offset_of!(TrapFrame, epc)) / core::mem::size_of::<usize>(),
                status = const (core::mem::offset_of!(SavedTrap, frame) + core::mem::offset_of!(TrapFrame, status)) / core::mem::size_of::<usize>(),
                status_high = const (core::mem::offset_of!(SavedTrap, frame) + core::mem::offset_of!(TrapFrame, status_high)) / core::mem::size_of::<usize>(),
                original_epc = const (core::mem::offset_of!(SavedTrap, info) + core::mem::offset_of!(TrapInfo, epc)) / core::mem::size_of::<usize>(),
                original_status = const (core::mem::offset_of!(SavedTrap, info) + core::mem::offset_of!(TrapInfo, status)) / core::mem::size_of::<usize>(),
                original_status_high = const (core::mem::offset_of!(SavedTrap, info) + core::mem::offset_of!(TrapInfo, status_high)) / core::mem::size_of::<usize>(),
                cause = const (core::mem::offset_of!(SavedTrap, info) + core::mem::offset_of!(TrapInfo, cause)) / core::mem::size_of::<usize>(),
                tval = const (core::mem::offset_of!(SavedTrap, info) + core::mem::offset_of!(TrapInfo, tval)) / core::mem::size_of::<usize>(),
                tval2 = const (core::mem::offset_of!(SavedTrap, info) + core::mem::offset_of!(TrapInfo, tval2)) / core::mem::size_of::<usize>(),
                instruction = const (core::mem::offset_of!(SavedTrap, info) + core::mem::offset_of!(TrapInfo, instruction)) / core::mem::size_of::<usize>(),
                hypervisor_offset = const (core::mem::offset_of!(SavedTrap, info) + core::mem::offset_of!(TrapInfo, has_hypervisor)) / core::mem::size_of::<usize>(),
                runtime_stack = const core::mem::offset_of!(SavedTrap, runtime_stack) / core::mem::size_of::<usize>(),
                scratch = const machine::Scratch::NUMBER,
                epc_csr = const machine::ExceptionPc::NUMBER,
                status_csr = const machine::Status::NUMBER,
                status_high_csr = const STATUS_HIGH_NUMBER,
                cause_csr = const machine::Cause::NUMBER,
                tval_csr = const machine::TrapValue::NUMBER,
                tval2_csr = const machine::TrapValue2::NUMBER,
                instruction_csr = const machine::TrapInstruction::NUMBER,
                clear_status = const (1usize << 17) | {
                    #[cfg(target_pointer_width = "64")]
                    { 1usize << 42 }
                    #[cfg(target_pointer_width = "32")]
                    { 0 }
                },
                mpp_mask = const 3 << 11,
                high = const $high,
                hypervisor = const $hypervisor,
                dispatch = sym super::dispatch::trap_dispatch,
            );
            #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
            panic!("trap entry requires RISC-V");
        }
    };
}

trap_vector!(trap_entry, 0, 0);
#[cfg(target_pointer_width = "32")]
trap_vector!(trap_entry_high, 1, 0);
#[cfg(target_pointer_width = "32")]
trap_vector!(trap_entry_hypervisor, 1, 1);
#[cfg(target_pointer_width = "64")]
trap_vector!(trap_entry_hypervisor, 0, 1);
