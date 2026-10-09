// Sstc: supervisor timer compare register.

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
}
