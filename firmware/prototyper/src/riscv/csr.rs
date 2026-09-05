#![allow(unused)]

use core::arch::asm;

use pastey::paste;
use seq_macro::seq;

use crate::sbi::early_trap::{
    TrapInfo, csr_read_allow, csr_swap, csr_write_allow, light_expected_trap,
};

// Sstc: supervisor timer compare register.
pub const CSR_STIMECMP: u16 = 0x14D;

// Machine counter-enable, environment-configuration, and state-enable CSRs.
pub const CSR_MCOUNTEREN: u16 = 0x306;
pub const CSR_MENVCFG: u16 = 0x30a;
pub const CSR_MSTATEEN0: u16 = 0x30c;
pub const CSR_MSTATEEN1: u16 = 0x30d;
pub const CSR_MSTATEEN2: u16 = 0x30e;
pub const CSR_MSTATEEN3: u16 = 0x30f;

// Machine counter setup (inhibit and event configuration).
pub const CSR_MCOUNTINHIBIT: u16 = 0x320;
pub const CSR_MCYCLECFG: u16 = 0x321;
pub const CSR_MINSTRETCFG: u16 = 0x322;
seq!(N in 3..32 {
    pub const CSR_MHPMEVENT~N: u16 = 0x320 + N;
});

// Machine Counter/Timers
pub const CSR_MCYCLE: u16 = 0xb00;
pub const CSR_MINSTRET: u16 = 0xb02;
seq!(N in 3..32 {
    pub const CSR_MHPMCOUNTER~N: u16 = 0xb00 + N;
});

// Upper 32 bits of Machine Counter/Timers (RV32)
pub const CSR_MCYCLEH: u16 = 0xb80;
pub const CSR_MINSTRETH: u16 = 0xb82;
seq!(N in 3..32 {
    paste! {
        pub const [<CSR_MHPMCOUNTER ~N H>]: u16 = 0xb80 + N;
    }
});

// User Counter/Timers (Read-only shadows of Machine counters)
pub const CSR_CYCLE: u16 = 0xc00;
pub const CSR_TIME: u16 = 0xc01;
pub const CSR_INSTRET: u16 = 0xc02;
seq!(N in 3..32 {
    pub const CSR_HPMCOUNTER~N: u16 = 0xc00 + N;
});

// Upper 32 bits of User Counter/Timers (RV32)
pub const CSR_CYCLEH: u16 = 0xc80;
pub const CSR_TIMEH: u16 = 0xc81;
pub const CSR_INSTRETH: u16 = 0xc82;
seq!(N in 3..32 {
    paste!{ pub const [<CSR_HPMCOUNTER ~N H>]: u16 = 0xc80 + N; }
});

/// Probes whether the CSR selected by `CSR` is implemented on this hart.
pub fn has_csr<const CSR: u16>() -> bool {
    use riscv::register::mtvec;

    let res: usize;
    // SAFETY: `mtvec` is backed up and restored around the probe, and
    // `light_expected_trap` skips the faulting `csrr`, so touching an
    // unimplemented CSR cannot escape into firmware. The a0-a2 clobbers are
    // part of the probe contract; this runs at init time only.
    unsafe {
        // Backup old mtvec
        let mtvec = mtvec::read().bits();
        // Write expected_trap
        mtvec::write(mtvec::Mtvec::new(
            light_expected_trap as *const () as _,
            mtvec::TrapMode::Direct,
        ));
        asm!("addi a0, zero, 0",
            "addi a1, zero, 0",
            "csrr a2, {}",
            "mv {}, a0",
            const CSR,
            out(reg) res,
            options(nomem));
        asm!("csrw mtvec, {}", in(reg) mtvec);
    }
    res == 0
}

/// Probes whether the `mhpmcounter` CSR selected by `CSR_NUM` (0xb03..=0xb1f)
/// exists and is writable, setting its bit in `mhpm_mask` when it does.
pub fn probe_mhpm_csr<const CSR_NUM: u16>(trap_info: &mut TrapInfo, mhpm_mask: &mut u32) {
    let trap_info = trap_info as *mut TrapInfo;
    // SAFETY: `trap_info` points to a live, owned `TrapInfo` for the whole
    // call; `csr_read_allow`/`csr_write_allow` install the expected-trap
    // vector in `mtvec` and restore the old vector before returning, so
    // touching a missing CSR is contained. `CSR_NUM` is a compile-time
    // mhpmcounter selector from the caller's `seq!` range.
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

/// Machine environment configuration register (menvcfg) bit fields.
pub mod menvcfg {
    use core::arch::asm;

    /// Fence of I/O implies memory.
    pub const FIOM: usize = 0x1 << 0;
    /// Cache-block-invalidate effect: flush (CBIE=01).
    pub const CBIE_FLUSH: usize = 0x01 << 4;
    /// Intended cache-block-invalidate effect: invalidate.
    pub const CBIE_INVALIDATE: usize = 0x11 << 4;
    /// Cache-block-clean flush enable.
    pub const CBCFE: usize = 0x1 << 6;
    /// Cache-block-zero enable.
    pub const CBZE: usize = 0x1 << 7;
    /// Page-based memory types enable.
    pub const PBMTE: usize = 0x1 << 62;
    /// Supervisor timer counter enable.
    pub const STCE: usize = 0x1 << 63;

    /// Sets the STCE bit to enable supervisor timer counter.
    #[inline(always)]
    pub fn set_stce() {
        set_bits(STCE);
    }

    /// Sets specified bits in menvcfg register.
    pub fn set_bits(option: usize) {
        let mut bits: usize;
        // SAFETY: M-mode read of this hart's own menvcfg.
        unsafe {
            asm!("csrr {}, menvcfg", out(reg) bits, options(nomem));
        }
        bits |= option;
        // SAFETY: M-mode write to this hart's own menvcfg.
        unsafe {
            asm!("csrw menvcfg, {}", in(reg) bits, options(nomem));
        }
    }
}

/// Machine state-enable register bit fields.
pub mod mstateen {
    use core::arch::asm;

    use super::{CSR_MSTATEEN0, CSR_MSTATEEN1, CSR_MSTATEEN2, CSR_MSTATEEN3};

    /// Counter delegation state.
    pub const CTR: usize = 1usize << 54;
    /// Context CSRs.
    pub const CONTEXT: usize = 1usize << 57;
    /// IMSIC state.
    pub const IMSIC: usize = 1usize << 58;
    /// AIA state.
    pub const AIA: usize = 1usize << 59;
    /// Supervisor indirect CSR select state.
    pub const SVSLCT: usize = 1usize << 60;
    /// Hypervisor environment configuration state.
    pub const HSENVCFG: usize = 1usize << 62;
    /// State-enable CSRs themselves.
    pub const STATEN: usize = 1usize << 63;

    /// Enables S-mode access to the AIA-related state groups in the
    /// `mstateen` CSRs.
    #[inline(always)]
    pub fn enable_smode_aia() {
        let stateen0 = STATEN | CONTEXT | IMSIC | AIA | SVSLCT | HSENVCFG | CTR;
        // SAFETY: M-mode writes to this hart's own state-enable registers.
        unsafe {
            asm!("csrw {csr}, {value}", csr = const CSR_MSTATEEN0, value = in(reg) stateen0, options(nomem));
            asm!("csrw {csr}, {value}", csr = const CSR_MSTATEEN1, value = in(reg) STATEN, options(nomem));
            asm!("csrw {csr}, {value}", csr = const CSR_MSTATEEN2, value = in(reg) STATEN, options(nomem));
            asm!("csrw {csr}, {value}", csr = const CSR_MSTATEEN3, value = in(reg) STATEN, options(nomem));
        }
    }
}

/// Machine interrupt-file CSR operations.
pub mod imsic {
    use riscv_aia::csrind::eidelivery::{self, Eidelivery};
    use riscv_aia::csrind::eie::{self, Eie};
    use riscv_aia::csrind::eip::{self, Eip};
    use riscv_aia::csrind::eithreshold::{self, Eithreshold};

    /// Initializes this hart's machine interrupt file and enables machine
    /// external interrupts.
    ///
    /// Callers probe Smaia before reaching this architecture boundary;
    /// selectors are derived from the validated IMSIC identity count.
    pub fn initialize_machine_file(num_ids: usize, ipi_iid: usize) {
        // SAFETY: the caller verified the current hart implements Smaia, and
        // M-mode firmware may access its own machine interrupt-file registers;
        // the selectors below come from the fixed IMSIC register map.
        unsafe {
            // Enable delivery from the interrupt file and clear the priority
            // threshold so every enabled identity can signal.
            eidelivery::machine::write(Eidelivery::ENABLED);
            eithreshold::machine::write(Eithreshold::from_bits(0));

            let num_regs = num_ids.div_ceil(32);
            for index in 0..num_regs {
                // On RV64 only even-numbered `eip`/`eie` registers exist.
                #[cfg(target_pointer_width = "64")]
                if index % 2 == 1 {
                    continue;
                }
                eip::machine::write(index, Eip::from_bits(0));
                eie::machine::write(index, Eie::from_bits(0));
            }

            // Enable the firmware IPI identity.
            #[cfg(target_pointer_width = "64")]
            let eie_index = (ipi_iid / 64) * 2;
            #[cfg(target_pointer_width = "32")]
            let eie_index = ipi_iid / 32;
            let bit_pos = ipi_iid % usize::BITS as usize;
            let enabled = eie::machine::read(eie_index).set_enabled(bit_pos as u32, true);
            eie::machine::write(eie_index, enabled);
        }

        super::mie::set_mext();
    }
}

/// Supervisor timer compare register operations.
pub mod stimecmp {
    use core::arch::asm;

    /// Sets the supervisor timer compare value.
    pub fn set(value: u64) {
        // SAFETY: callers have probed Sstc; M-mode may program stimecmp on
        // this hart when the extension is implemented.
        unsafe {
            asm!("csrrw zero, stimecmp, {}", in(reg) value, options(nomem));
        }
    }
}

/// Machine cycle counter.
pub mod mcycle {
    use core::arch::asm;
    /// Writes the raw 64-bit counter value.
    pub fn write(value: u64) {
        // SAFETY: M-mode write to this hart's own counter.
        unsafe {
            asm!("csrrw zero, mcycle, {}", in(reg) value, options(nomem));
        }
    }
}

/// Machine instructions-retired counter.
pub mod minstret {
    use core::arch::asm;
    /// Writes the raw 64-bit counter value.
    pub fn write(value: u64) {
        // SAFETY: M-mode write to this hart's own counter.
        unsafe {
            asm!("csrrw zero, minstret, {}", in(reg) value, options(nomem));
        }
    }
}

/// Machine interrupt-enable (`mie`) bit operations.
pub mod mie {
    use riscv::register::mie;

    /// Enables the machine software interrupt.
    pub fn enable_msoft() {
        // SAFETY: M-mode firmware toggling its own interrupt-enable bits.
        unsafe { mie::set_msoft() }
    }

    /// Disables the machine software interrupt.
    pub fn disable_msoft() {
        // SAFETY: M-mode firmware toggling its own interrupt-enable bits.
        unsafe { mie::clear_msoft() }
    }

    /// Enables the machine timer interrupt.
    pub fn set_mtimer() {
        // SAFETY: M-mode firmware toggling its own interrupt-enable bits.
        unsafe { mie::set_mtimer() }
    }

    /// Disables the machine timer interrupt.
    pub fn clear_mtimer() {
        // SAFETY: M-mode firmware toggling its own interrupt-enable bits.
        unsafe { mie::clear_mtimer() }
    }

    /// Enables the machine external interrupt.
    pub fn set_mext() {
        // SAFETY: M-mode firmware toggling its own interrupt-enable bits.
        unsafe { mie::set_mext() }
    }
}

/// Machine interrupt-pending (`mip`) bit operations.
pub mod mip {
    use riscv::register::mip;

    /// Sets the supervisor software interrupt pending bit.
    pub fn set_ssoft() {
        // SAFETY: M-mode may write mip.SSIP; the bit only signals an S-mode
        // software interrupt.
        unsafe { mip::set_ssoft() }
    }

    /// Sets the supervisor timer interrupt pending bit.
    pub fn set_stimer() {
        // SAFETY: M-mode CSR write; the bit only signals the S-mode timer
        // interrupt.
        unsafe { mip::set_stimer() }
    }

    /// Clears the supervisor timer interrupt pending bit.
    pub fn clear_stimer() {
        // SAFETY: M-mode CSR write; the bit only signals the S-mode timer
        // interrupt. Writes are ignored when Sstc drives STIP.
        unsafe { mip::clear_stimer() }
    }
}

/// Machine counter-inhibit (`mcountinhibit`) operations.
pub mod mcountinhibit {
    use core::arch::asm;
    use riscv::register::mcountinhibit;

    /// Reads the raw `mcountinhibit` value.
    pub fn read() -> usize {
        mcountinhibit::read().bits()
    }

    /// Overwrites `mcountinhibit` with `bits`.
    pub fn write_raw(bits: usize) {
        // SAFETY: M-mode init; inhibit bits only gate PMU accounting.
        unsafe { asm!("csrw mcountinhibit, {}", in(reg) bits) };
    }

    /// Inhibits the cycle counter.
    pub fn set_cy() {
        // SAFETY: M-mode; inhibit bits only gate PMU accounting.
        unsafe { mcountinhibit::set_cy() }
    }

    /// Re-enables the cycle counter.
    pub fn clear_cy() {
        // SAFETY: M-mode; inhibit bits only gate PMU accounting.
        unsafe { mcountinhibit::clear_cy() }
    }

    /// Inhibits the instret counter.
    pub fn set_ir() {
        // SAFETY: M-mode; inhibit bits only gate PMU accounting.
        unsafe { mcountinhibit::set_ir() }
    }

    /// Re-enables the instret counter.
    pub fn clear_ir() {
        // SAFETY: M-mode; inhibit bits only gate PMU accounting.
        unsafe { mcountinhibit::clear_ir() }
    }

    /// Inhibits `mhpmcounterN` for `index` (3..=31).
    pub fn set_hpm(index: usize) {
        // SAFETY: M-mode; callers pass an `index` in 3..=31 for a counter
        // this hart reported in `mhpm_mask`.
        unsafe { mcountinhibit::set_hpm(index) }
    }

    /// Re-enables `mhpmcounterN` for `index` (3..=31).
    pub fn clear_hpm(index: usize) {
        // SAFETY: M-mode; callers pass an `index` in 3..=31 for a counter
        // this hart reported in `mhpm_mask`.
        unsafe { mcountinhibit::clear_hpm(index) }
    }
}

/// Writes `mhpmeventN` for the counter at `mhpm_offset` (3..=31).
pub fn write_mhpmevent(mhpm_offset: u16, mhpmevent_val: u64) {
    use riscv::register::*;

    let csr = CSR_MHPMEVENT3 + mhpm_offset - 3;

    if csr >= CSR_MHPMEVENT3 && csr <= CSR_MHPMEVENT31 {
        let idx = csr - CSR_MHPMEVENT3 + 3;

        seq_macro::seq!(N in 3..=31 {
            match idx {
                #(
                    // SAFETY: M-mode; `idx` is in 3..=31 and the probed
                    // `mhpm_mask` guarantees the selector exists on this hart.
                    N => unsafe {
                        pastey::paste!{ [<mhpmevent ~N>]::write(mhpmevent_val as usize) }
                    },
                )*
                _ =>{}
            }
        });
    }
}

/// Writes `mhpmcounterN` (or `mcycle`/`minstret`) for `mhpm_offset`.
pub fn write_mhpmcounter(mhpm_offset: u16, mhpmcounter_val: u64) {
    use riscv::register::*;

    let counter_idx = mhpm_offset;

    let csr = CSR_MHPMCOUNTER3 + mhpm_offset - 3;
    if csr == CSR_MCYCLE {
        self::mcycle::write(mhpmcounter_val);
        return;
    } else if csr == CSR_MINSTRET {
        self::minstret::write(mhpmcounter_val);
        return;
    }

    // Only counter indices 3..=31 name an `mhpmcounter` register.
    if counter_idx >= 3 && counter_idx <= 31 {
        seq_macro::seq!(N in 3..=31 {
            match counter_idx {
                #(
                    // SAFETY: M-mode; `counter_idx` is in 3..=31 and the probed
                    // `mhpm_mask` guarantees the counter exists on this hart.
                    N => pastey::paste!{ unsafe {
                        [<mhpmcounter ~N>]::write(mhpmcounter_val as usize) }
                    },
                )*
                _ =>{}
            }
        });
    }
}

/// Delegates interrupts, exceptions, and counters to supervisor mode, while
/// keeping supervisor ecalls and misaligned/illegal instructions in M-mode.
///
/// The body is the firmware's fixed delegation policy; it runs once per hart
/// during M-mode init, before any supervisor code executes.
pub fn configure_delegation() {
    use riscv::register::medeleg;

    // SAFETY: M-mode init on the current hart; the written values are the
    // firmware's fixed delegation policy and have no memory-safety impact.
    unsafe {
        asm!("csrw mideleg,    {}", in(reg) !0);
        asm!("csrw medeleg,    {}", in(reg) !0);
        asm!("csrw mcounteren, {}", in(reg) !0);
        asm!("csrw scounteren, {}", in(reg) !0);
        medeleg::clear_supervisor_env_call();
        medeleg::clear_load_misaligned();
        medeleg::clear_store_misaligned();
        medeleg::clear_illegal_instruction();
    }
}

/// Installs the fast-trap entry as the machine trap vector (direct mode).
///
/// Runs once per hart during M-mode init, after delegation is configured.
pub fn install_trap_vector() {
    use riscv::register::mtvec;

    let val = mtvec::Mtvec::new(
        fast_trap::trap_entry as *const () as _,
        mtvec::TrapMode::Direct,
    );
    // SAFETY: `fast_trap::trap_entry` is a valid, aligned M-mode trap entry
    // for direct mode.
    unsafe { mtvec::write(val) }
}

/// Fence instruction family (`fence.i`, `sfence.vma`, and the
/// hypervisor-gated `hfence.gvma` / `hfence.vvma`).
pub mod fence {
    use core::arch::asm;

    /// Fences instruction fetch for the current hart (`fence.i`).
    pub fn fence_i() {
        // SAFETY: instruction-fetch ordering on the local hart only.
        unsafe { asm!("fence.i") };
    }

    /// Invalidates all supervisor TLB entries (`sfence.vma`).
    pub fn sfence_vma_all() {
        // SAFETY: full TLB invalidate; requested by a validated SBI rfence call.
        unsafe { asm!("sfence.vma") };
    }

    /// Invalidates supervisor TLB entries for `addr` (`sfence.vma addr`).
    pub fn sfence_vma_addr(addr: usize) {
        // SAFETY: partial TLB invalidate; operands come from a validated
        // SBI rfence call.
        unsafe { asm!("sfence.vma {}", in(reg) addr) };
    }

    /// Invalidates all supervisor TLB entries for `asid`
    /// (`sfence.vma x0, asid`).
    pub fn sfence_vma_asid(asid: usize) {
        // SAFETY: per-ASID TLB invalidate; requested by a validated SBI rfence call.
        unsafe { asm!("sfence.vma x0, {}", in(reg) asid) };
    }

    /// Invalidates supervisor TLB entries for (`addr`, `asid`)
    /// (`sfence.vma addr, asid`).
    pub fn sfence_vma_addr_asid(addr: usize, asid: usize) {
        // SAFETY: partial, per-ASID TLB invalidate; operands come from a
        // validated SBI rfence call.
        unsafe { asm!("sfence.vma {}, {}", in(reg) addr, in(reg) asid) };
    }

    /// Invalidates all guest TLB entries (`hfence.gvma x0, x0`).
    #[cfg(feature = "hypervisor")]
    pub fn hfence_gvma_all() {
        // SAFETY: guest-TLB invalidate; the hypervisor extension probe gates
        // every call site.
        unsafe { asm!("hfence.gvma x0, x0") };
    }

    /// Invalidates guest TLB entries for `addr` (`hfence.gvma addr, x0`).
    #[cfg(feature = "hypervisor")]
    pub fn hfence_gvma_addr(addr: usize) {
        // SAFETY: single-page guest-physical TLB invalidate; the hypervisor
        // extension probe gates every call site.
        unsafe { asm!("hfence.gvma {}, x0", in(reg) addr) };
    }

    /// Invalidates all guest TLB entries for `vmid` (`hfence.gvma x0, vmid`).
    #[cfg(feature = "hypervisor")]
    pub fn hfence_gvma_vmid(vmid: usize) {
        // SAFETY: per-VMID guest-physical TLB invalidate; the hypervisor
        // extension probe gates every call site.
        unsafe { asm!("hfence.gvma x0, {}", in(reg) vmid) };
    }

    /// Invalidates guest TLB entries for (`addr`, `vmid`)
    /// (`hfence.gvma addr, vmid`).
    #[cfg(feature = "hypervisor")]
    pub fn hfence_gvma_addr_vmid(addr: usize, vmid: usize) {
        // SAFETY: partial, per-VMID guest-physical TLB invalidate; the
        // hypervisor extension probe gates every call site.
        unsafe { asm!("hfence.gvma {}, {}", in(reg) addr, in(reg) vmid) };
    }

    /// Invalidates all guest supervisor TLB entries (`hfence.vvma x0, x0`).
    #[cfg(feature = "hypervisor")]
    pub fn hfence_vvma_all() {
        // SAFETY: guest-TLB invalidate; the hypervisor extension probe gates
        // every call site.
        unsafe { asm!("hfence.vvma x0, x0") };
    }

    /// Invalidates guest supervisor TLB entries for `addr`
    /// (`hfence.vvma addr, x0`).
    #[cfg(feature = "hypervisor")]
    pub fn hfence_vvma_addr(addr: usize) {
        // SAFETY: single-page guest-virtual TLB invalidate; the hypervisor
        // extension probe gates every call site.
        unsafe { asm!("hfence.vvma {}, x0", in(reg) addr) };
    }

    /// Invalidates all guest supervisor TLB entries for `asid`
    /// (`hfence.vvma x0, asid`).
    #[cfg(feature = "hypervisor")]
    pub fn hfence_vvma_asid(asid: usize) {
        // SAFETY: per-ASID guest-virtual TLB invalidate; the hypervisor
        // extension probe gates every call site.
        unsafe { asm!("hfence.vvma x0, {}", in(reg) asid) };
    }

    /// Invalidates guest supervisor TLB entries for (`addr`, `asid`)
    /// (`hfence.vvma addr, asid`).
    #[cfg(feature = "hypervisor")]
    pub fn hfence_vvma_addr_asid(addr: usize, asid: usize) {
        // SAFETY: partial, per-ASID guest-virtual TLB invalidate; the
        // hypervisor extension probe gates every call site.
        unsafe { asm!("hfence.vvma {}, {}", in(reg) addr, in(reg) asid) };
    }
}
