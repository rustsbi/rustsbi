#[cfg(target_pointer_width = "32")]
use core::arch::asm;
use csr::machine;

// Machine counter inhibit and the event-selector CSR range.
pub const CSR_MHPMEVENT3: u16 = 0x323;
pub const CSR_MHPMEVENT31: u16 = 0x33f;

// Machine counters and the base of their read-only user shadows.
pub const CSR_MCYCLE: u16 = 0xb00;
pub const CSR_MINSTRET: u16 = 0xb02;
pub const CSR_MHPMCOUNTER3: u16 = 0xb03;
pub const CSR_CYCLE: u16 = 0xc00;

/// Probes whether the current hart can read a declared CSR.
///
/// # Safety
///
/// Satisfy [`runtime::csr::read`]'s entry and register-specific requirements.
pub unsafe fn has_csr<R: csr::Readable>() -> bool {
    // SAFETY: The caller establishes this CSR's read and entry requirements.
    unsafe { runtime::csr::read::<R>() }.is_ok()
}

/// Probes the machine performance counter at `INDEX` (3..=31).
///
/// # Safety
///
/// Run during M-mode feature discovery after Runtime entry is installed,
/// with `mscratch` zero, `MPRV` clear, and exclusive access to this counter.
pub unsafe fn probe_mhpm_csr<const INDEX: u8>(mhpm_mask: &mut u32) {
    // SAFETY: The caller owns this counter during Runtime feature discovery.
    if let Ok(old_value) = unsafe { runtime::csr::read::<machine::PerformanceCounter<INDEX>>() }
        // SAFETY: The probe still owns this counter and may change its value.
        && unsafe { runtime::csr::write::<machine::PerformanceCounter<INDEX>>(1usize.into()) }.is_ok()
        // SAFETY: The probe retains exclusive access and supplies the saved counter value.
        && unsafe { runtime::csr::swap::<machine::PerformanceCounter<INDEX>>(old_value) }.map(usize::from)
            == Ok(1)
    {
        *mhpm_mask |= 1 << INDEX;
    }
}

/// Machine environment configuration register (`menvcfg`) bit fields.
pub mod menvcfg {
    use core::arch::asm;

    /// Cache-block-invalidate effect: invalidate (CBIE=11).
    pub const CBIE_INVALIDATE: u64 = 0b11 << 4;
    /// Cache-block-clean flush enable.
    pub const CBCFE: u64 = 0x1 << 6;
    /// Cache-block-zero enable.
    pub const CBZE: u64 = 0x1 << 7;
    /// Page-based memory types enable.
    pub const PBMTE: u64 = 0x1 << 62;
    /// Supervisor timer counter enable.
    pub const STCE: u64 = 0x1 << 63;

    /// Clears specified bits in `menvcfg`.
    pub fn clear_bits(option: u64) {
        // SAFETY: M-mode update of this hart's own menvcfg.
        unsafe {
            asm!("csrc menvcfg, {}", in(reg) option as usize, options(nomem));
            #[cfg(target_pointer_width = "32")]
            if option >> 32 != 0 {
                asm!("csrc 0x31a, {}", in(reg) (option >> 32) as usize, options(nomem));
            }
        }
    }

    /// Sets specified bits in `menvcfg` register.
    pub fn set_bits(option: u64) {
        // SAFETY: M-mode update of this hart's own menvcfg. On RV32 the
        // upper half is menvcfgh (0x31a); callers probe the extension before
        // requesting its high bits.
        unsafe {
            asm!("csrs menvcfg, {}", in(reg) option as usize, options(nomem));
            #[cfg(target_pointer_width = "32")]
            if option >> 32 != 0 {
                asm!("csrs 0x31a, {}", in(reg) (option >> 32) as usize, options(nomem));
            }
        }
    }
}

/// Machine security configuration register (`mseccfg`) bit fields.
pub mod mseccfg {
    use core::arch::asm;

    /// S-mode access to the `seed` CSR (Zkr).
    pub const SSEED: u64 = 0x1 << 9;

    /// Sets specified bits in `mseccfg` register.
    pub fn set_bits(option: u64) {
        // SAFETY: M-mode update of this hart's own mseccfg. Callers probe the
        // CSR first; the Zkr bits live in the low 32 bits on every XLEN, so
        // no high-half write is needed on RV32.
        unsafe {
            asm!("csrs mseccfg, {}", in(reg) option as usize, options(nomem));
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
        // SAFETY: The caller verified the current hart implements Smaia, and
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

        runtime::csr::mie::set_machine_external();
    }
}

/// Supervisor timer compare register operations.
pub mod stimecmp {
    use core::arch::asm;

    /// Sets the supervisor timer compare value.
    pub fn set(value: u64) {
        // SAFETY: Callers have probed Sstc; M-mode may program stimecmp on
        // this hart when the extension is implemented.
        unsafe {
            #[cfg(target_pointer_width = "64")]
            asm!("csrrw zero, stimecmp, {}", in(reg) value, options(nomem));
            // Avoid an early interrupt while replacing the two halves.
            #[cfg(target_pointer_width = "32")]
            asm!(
                "csrw stimecmp, {max}",
                "csrw 0x15d, {high}",
                "csrw stimecmp, {low}",
                max = in(reg) usize::MAX,
                high = in(reg) (value >> 32) as usize,
                low = in(reg) value as usize,
                options(nomem),
            );
        }
    }
}

/// Machine cycle counter.
pub mod mcycle {
    /// Writes the raw 64-bit counter value.
    pub fn write(value: u64) {
        // SAFETY: M-mode write to this hart's own counter.
        unsafe {
            riscv::register::mcycle::write64(value);
        }
    }
}

/// Machine instructions-retired counter.
pub mod minstret {
    /// Writes the raw 64-bit counter value.
    pub fn write(value: u64) {
        // SAFETY: M-mode write to this hart's own counter.
        unsafe {
            riscv::register::minstret::write64(value);
        }
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

    if (CSR_MHPMEVENT3..=CSR_MHPMEVENT31).contains(&csr) {
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
    if (3..=31).contains(&counter_idx) {
        seq_macro::seq!(N in 3..=31 {
            match counter_idx {
                #(
                    // SAFETY: M-mode; `counter_idx` is in 3..=31 and the probed
                    // `mhpm_mask` guarantees the counter exists on this hart.
                    N => pastey::paste!{ unsafe {
                        #[cfg(target_pointer_width = "32")]
                        asm!("csrw {csr}, {value}",
                            csr = const 0xb80 + N,
                            value = in(reg) (mhpmcounter_val >> 32) as usize,
                            options(nomem));
                        [<mhpmcounter ~N>]::write(mhpmcounter_val as usize) }
                    },
                )*
                _ =>{}
            }
        });
    }
}

/// Memory ordering and address-translation fence instructions.
pub mod fence {
    use core::arch::asm;

    /// Publishes shared-memory writes before notifying an I/O device.
    #[inline]
    pub fn memory_to_io() {
        // SAFETY: Orders memory writes before device output on this hart.
        unsafe { asm!("fence w, o", options(nostack)) };
    }

    /// Orders device acknowledgement before accessing shared event state.
    #[inline]
    pub fn io_to_memory() {
        // SAFETY: Orders MMIO/CSR acknowledgement before memory accesses.
        unsafe { asm!("fence io, rw", options(nostack)) };
    }

    /// Invalidates all supervisor TLB entries (`sfence.vma`).
    pub fn sfence_vma_all() {
        // SAFETY: Full TLB invalidate; requested by a validated SBI rfence call.
        unsafe { asm!("sfence.vma") };
    }

    /// Invalidates supervisor TLB entries for `addr` (`sfence.vma addr`).
    pub fn sfence_vma_addr(addr: usize) {
        // SAFETY: Partial TLB invalidate; operands come from a validated
        // SBI rfence call.
        unsafe { asm!("sfence.vma {}", in(reg) addr) };
    }

    /// Invalidates all supervisor TLB entries for `asid`
    /// (`sfence.vma x0, asid`).
    pub fn sfence_vma_asid(asid: usize) {
        // SAFETY: Per-ASID TLB invalidate; requested by a validated SBI rfence call.
        unsafe { asm!("sfence.vma x0, {}", in(reg) asid) };
    }

    /// Invalidates supervisor TLB entries for (`addr`, `asid`)
    /// (`sfence.vma addr, asid`).
    pub fn sfence_vma_addr_asid(addr: usize, asid: usize) {
        // SAFETY: Partial, per-ASID TLB invalidate; operands come from a
        // validated SBI rfence call.
        unsafe { asm!("sfence.vma {}, {}", in(reg) addr, in(reg) asid) };
    }

    /// Invalidates all guest TLB entries (`hfence.gvma x0, x0`).
    #[cfg(feature = "hypervisor")]
    pub fn hfence_gvma_all() {
        // SAFETY: Guest-TLB invalidate; the hypervisor extension probe gates
        // every call site.
        unsafe { asm!("hfence.gvma x0, x0") };
    }

    /// Invalidates guest TLB entries for `addr` (`hfence.gvma addr, x0`).
    #[cfg(feature = "hypervisor")]
    pub fn hfence_gvma_addr(addr: usize) {
        // SAFETY: Single-page guest-physical TLB invalidate; the hypervisor
        // extension probe gates every call site.
        unsafe { asm!("hfence.gvma {}, x0", in(reg) addr) };
    }

    /// Invalidates all guest TLB entries for `vmid` (`hfence.gvma x0, vmid`).
    #[cfg(feature = "hypervisor")]
    pub fn hfence_gvma_vmid(vmid: usize) {
        // SAFETY: Per-VMID guest-physical TLB invalidate; the hypervisor
        // extension probe gates every call site.
        unsafe { asm!("hfence.gvma x0, {}", in(reg) vmid) };
    }

    /// Invalidates guest TLB entries for (`addr`, `vmid`)
    /// (`hfence.gvma addr, vmid`).
    #[cfg(feature = "hypervisor")]
    pub fn hfence_gvma_addr_vmid(addr: usize, vmid: usize) {
        // SAFETY: Partial, per-VMID guest-physical TLB invalidate; the
        // hypervisor extension probe gates every call site.
        unsafe { asm!("hfence.gvma {}, {}", in(reg) addr, in(reg) vmid) };
    }

    /// Invalidates all guest supervisor TLB entries (`hfence.vvma x0, x0`).
    #[cfg(feature = "hypervisor")]
    pub fn hfence_vvma_all() {
        // SAFETY: Guest-TLB invalidate; the hypervisor extension probe gates
        // every call site.
        unsafe { asm!("hfence.vvma x0, x0") };
    }

    /// Invalidates guest supervisor TLB entries for `addr`
    /// (`hfence.vvma addr, x0`).
    #[cfg(feature = "hypervisor")]
    pub fn hfence_vvma_addr(addr: usize) {
        // SAFETY: Single-page guest-virtual TLB invalidate; the hypervisor
        // extension probe gates every call site.
        unsafe { asm!("hfence.vvma {}, x0", in(reg) addr) };
    }

    /// Invalidates all guest supervisor TLB entries for `asid`
    /// (`hfence.vvma x0, asid`).
    #[cfg(feature = "hypervisor")]
    pub fn hfence_vvma_asid(asid: usize) {
        // SAFETY: Per-ASID guest-virtual TLB invalidate; the hypervisor
        // extension probe gates every call site.
        unsafe { asm!("hfence.vvma x0, {}", in(reg) asid) };
    }

    /// Invalidates guest supervisor TLB entries for (`addr`, `asid`)
    /// (`hfence.vvma addr, asid`).
    #[cfg(feature = "hypervisor")]
    pub fn hfence_vvma_addr_asid(addr: usize, asid: usize) {
        // SAFETY: Partial, per-ASID guest-virtual TLB invalidate; the
        // hypervisor extension probe gates every call site.
        unsafe { asm!("hfence.vvma {}, {}", in(reg) addr, in(reg) asid) };
    }
}
