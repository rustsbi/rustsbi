//! Machine trap entry and return with integer-register preservation.
use core::mem::offset_of;
use riscv_csr_spec::standard::isa::privileged::machine::{
    Mcause, Mepc, Mscratch, Mstatus, Mtinst, Mtval, Mtval2,
};

use super::frame::SavedTrap;

pub(crate) const FRAME_BYTES: usize = core::mem::size_of::<SavedTrap>().next_multiple_of(16);

// Share the layout and restore sequence across optional-CSR variants.
// Boot selects a variant only after establishing its CSR availability.
macro_rules! trap_vector {
    ($name:ident, $hypervisor:literal $(, $high_field:ident = $save_high:literal)?) => {
        /// Handles a hardware machine trap using Runtime's stack protocol.
        ///
        /// # Safety
        ///
        /// 1. Enter through a hardware trap on a hart implementing the optional
        ///    CSRs used by this vector.
        /// 2. During Runtime execution, `mscratch` must be zero and `sp` must
        ///    address the hart's Runtime stack. During payload execution,
        ///    `mscratch` must hold the clean Runtime stack top.
        /// 3. The selected Runtime stack must be 16-byte aligned and have room
        ///    for the trap frame and the Rust handler's calls.
        #[unsafe(naked)]
        pub(crate) unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                ".balign 4",
                // A zero after the swap identifies a nested Runtime trap, which
                // must extend the interrupted stack instead of reusing its top.
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
                $(
                    concat!(".if ", $save_high),
                    "csrr t0, {status_high_csr}",
                    ".else",
                    "li t0, 0",
                    ".endif",
                    store!("t0", "{status_high}", "sp"),
                    store!("t0", "{original_status_high}", "sp"),
                )?
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
                $(
                    concat!(".if ", $save_high),
                    "li t0, {mdt_high}",
                    "csrc {status_high_csr}, t0",
                    ".endif",
                )?
                "csrw {scratch}, zero",
                "mv a0, sp",
                "call {dispatch}",
                // Restore return state from the frame; nested traps may change live CSRs.
                load!("t0", "{epc}", "sp"),
                "csrw {epc_csr}, t0",
                $(
                    concat!(".if ", $save_high),
                    load!("t0", "{status_high}", "sp"),
                    "csrw {status_high_csr}, t0",
                    ".endif",
                )?
                load!("t0", "{status}", "sp"),
                // A return to an M-mode payload republishes its Runtime stack;
                // a nested Runtime return keeps mscratch zero.
                load!("t1", "{runtime_stack}", "sp"),
                "li t2, 3 << 11", // mstatus.MPP = M-mode
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
                epc = const offset_of!(SavedTrap, frame.epc) / size_of::<usize>(),
                status = const offset_of!(SavedTrap, frame.status) / size_of::<usize>(),
                original_epc = const offset_of!(SavedTrap, info.epc) / size_of::<usize>(),
                original_status = const offset_of!(SavedTrap, info.status) / size_of::<usize>(),
                $(
                    status_high = const offset_of!(SavedTrap, frame.$high_field) / size_of::<usize>(),
                    original_status_high = const offset_of!(SavedTrap, info.$high_field) / size_of::<usize>(),
                    status_high_csr = const Mstatush::NUMBER,
                    mdt_high = const Mstatush::MDT.mask(),
                )?
                cause = const offset_of!(SavedTrap, info.cause) / size_of::<usize>(),
                tval = const offset_of!(SavedTrap, info.tval) / size_of::<usize>(),
                tval2 = const offset_of!(SavedTrap, info.tval2) / size_of::<usize>(),
                instruction = const offset_of!(SavedTrap, info.instruction) / size_of::<usize>(),
                hypervisor_offset = const offset_of!(SavedTrap, info.has_hypervisor) / size_of::<usize>(),
                runtime_stack = const offset_of!(SavedTrap, runtime_stack) / size_of::<usize>(),
                scratch = const Mscratch::NUMBER,
                epc_csr = const Mepc::NUMBER,
                status_csr = const Mstatus::NUMBER,
                cause_csr = const Mcause::NUMBER,
                tval_csr = const Mtval::NUMBER,
                tval2_csr = const Mtval2::NUMBER,
                instruction_csr = const Mtinst::NUMBER,
                clear_status = const CLEAR_STATUS,
                hypervisor = const $hypervisor,
                dispatch = sym super::dispatch::trap_dispatch,
            );
        }
    };
}

pub(crate) fn is_runtime_entry(base: usize) -> bool {
    #[cfg(target_arch = "riscv32")]
    if base == trap_entry_high as *const () as usize {
        return true;
    }
    base == trap_entry as *const () as usize || base == trap_entry_hypervisor as *const () as usize
}

cfg_select! {
    target_arch = "riscv32" => {
        use riscv_csr_spec::standard::isa::privileged::machine::Mstatush;
        const CLEAR_STATUS: usize = Mstatus::MPRV.mask();

        macro_rules! store {
            ($reg:literal, $offset:literal, $base:literal) => {
                concat!("sw ", $reg, ", 4*(", $offset, ")(", $base, ")")
            };
        }
        macro_rules! load {
            ($reg:literal, $offset:literal, $base:literal) => {
                concat!("lw ", $reg, ", 4*(", $offset, ")(", $base, ")")
            };
        }

        trap_vector!(trap_entry, 0, status_high = 0);
        trap_vector!(trap_entry_high, 0, status_high = 1);
        trap_vector!(trap_entry_hypervisor, 1, status_high = 1);
    }
    target_arch = "riscv64" => {
        const CLEAR_STATUS: usize = Mstatus::MPRV.mask() | Mstatus::MDT.mask();

        macro_rules! store {
            ($reg:literal, $offset:literal, $base:literal) => {
                concat!("sd ", $reg, ", 8*(", $offset, ")(", $base, ")")
            };
        }
        macro_rules! load {
            ($reg:literal, $offset:literal, $base:literal) => {
                concat!("ld ", $reg, ", 8*(", $offset, ")(", $base, ")")
            };
        }

        trap_vector!(trap_entry, 0);
        trap_vector!(trap_entry_hypervisor, 1);
    }
}
