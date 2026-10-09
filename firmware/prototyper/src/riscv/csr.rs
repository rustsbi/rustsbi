// Sstc: supervisor timer compare register.
pub const CSR_STIMECMP: u16 = 0x14D;

/// Probes whether the CSR selected by `CSR` is implemented on this hart.
pub fn has_csr<const CSR: u16>() -> bool {
    runtime::trap::read_csr_guarded::<CSR>().is_ok()
}

/// Machine environment configuration register (menvcfg) bit fields.
pub mod menvcfg {
    use core::arch::asm;
    /// Supervisor timer counter enable.
    pub const STCE: u64 = 0x1 << 63;

    /// Sets specified bits in menvcfg register.
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

        runtime::csr::mie::set_machine_external();
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

/// Memory ordering and address-translation fence instructions.
pub mod fence {
    use core::arch::asm;

    /// Publishes shared-memory writes before notifying an I/O device.
    #[inline]
    pub fn memory_to_io() {
        // SAFETY: orders memory writes before device output on this hart.
        unsafe { asm!("fence w, o", options(nostack)) };
    }

    /// Orders device acknowledgement before accessing shared event state.
    #[inline]
    pub fn io_to_memory() {
        // SAFETY: orders MMIO/CSR acknowledgement before memory accesses.
        unsafe { asm!("fence io, rw", options(nostack)) };
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
