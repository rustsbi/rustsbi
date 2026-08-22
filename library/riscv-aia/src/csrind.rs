//! Indirect CSRs for RISC-V AIA.

pub mod eidelivery;
pub mod eie;
pub mod eip;
pub mod eithreshold;

/// M-mode accessors for the machine-level interrupt file.
mod machine {
    use crate::register::{
        mireg::{self, Mireg},
        miselect::{self, Miselect},
    };
    use riscv::interrupt::machine as interrupt;

    /// Reads an indirectly accessed machine-level interrupt-file register.
    ///
    /// # Safety
    ///
    /// The current hart must implement Smaia, the caller must be permitted to access
    /// M-mode CSRs, and `reg_id` must select an implemented register for the current
    /// XLEN. Otherwise, the indirect CSR access may raise an illegal-instruction
    /// exception.
    #[inline]
    pub(super) unsafe fn read_ind(reg_id: usize) -> usize {
        interrupt::free(|| unsafe {
            let previous = miselect::read();
            miselect::write(Miselect::from_bits(reg_id));
            let value = mireg::read().bits();
            miselect::write(previous);
            value
        })
    }

    /// Writes an indirectly accessed machine-level interrupt-file register.
    ///
    /// # Safety
    ///
    /// The current hart must implement Smaia, the caller must be permitted to access
    /// M-mode CSRs, and `reg_id` must select an implemented register for the current
    /// XLEN. Otherwise, the indirect CSR access may raise an illegal-instruction
    /// exception.
    #[inline]
    pub(super) unsafe fn write_ind(reg_id: usize, value: usize) {
        interrupt::free(|| unsafe {
            let previous = miselect::read();
            miselect::write(Miselect::from_bits(reg_id));
            mireg::write(Mireg::from_bits(value));
            miselect::write(previous);
        })
    }
}

/// S-mode accessors for the supervisor-level interrupt file.
mod supervisor {
    use crate::register::{
        sireg::{self, Sireg},
        siselect::{self, Siselect},
    };
    use riscv::interrupt::supervisor as interrupt;

    /// Reads an indirectly accessed supervisor-level interrupt-file register.
    ///
    /// # Safety
    ///
    /// The current hart must implement Ssaia, the caller must be permitted to access
    /// S-mode CSRs, and `reg_id` must select an implemented register for the current
    /// XLEN. Otherwise, the indirect CSR access may raise an illegal-instruction
    /// exception.
    #[inline]
    pub(super) unsafe fn read_ind(reg_id: usize) -> usize {
        interrupt::free(|| unsafe {
            let previous = siselect::read();
            siselect::write(Siselect::from_bits(reg_id));
            let value = sireg::read().bits();
            siselect::write(previous);
            value
        })
    }

    /// Writes an indirectly accessed supervisor-level interrupt-file register.
    ///
    /// # Safety
    ///
    /// The current hart must implement Ssaia, the caller must be permitted to access
    /// S-mode CSRs, and `reg_id` must select an implemented register for the current
    /// XLEN. Otherwise, the indirect CSR access may raise an illegal-instruction
    /// exception.
    #[inline]
    pub(super) unsafe fn write_ind(reg_id: usize, value: usize) {
        interrupt::free(|| unsafe {
            let previous = siselect::read();
            siselect::write(Siselect::from_bits(reg_id));
            sireg::write(Sireg::from_bits(value));
            siselect::write(previous);
        })
    }
}

/// VS-mode accessors for the current guest interrupt file.
mod guest {
    use crate::register::{
        vsireg::{self, Vsireg},
        vsiselect::{self, Vsiselect},
    };
    use riscv::interrupt::supervisor as interrupt;

    /// Reads an indirectly accessed guest interrupt-file register.
    ///
    /// # Safety
    ///
    /// The current hart must implement the H extension with a guest interrupt file,
    /// the caller must be permitted to access VS-mode CSRs, `hstatus.VGEIN` must
    /// select an implemented guest interrupt file, and `reg_id` must select an
    /// implemented register for the current XLEN. Otherwise, the indirect CSR access
    /// may raise an illegal- or virtual-instruction exception.
    #[inline]
    pub(super) unsafe fn read_ind(reg_id: usize) -> usize {
        interrupt::free(|| unsafe {
            let previous = vsiselect::read();
            vsiselect::write(Vsiselect::from_bits(reg_id));
            let value = vsireg::read().bits();
            vsiselect::write(previous);
            value
        })
    }

    /// Writes an indirectly accessed guest interrupt-file register.
    ///
    /// # Safety
    ///
    /// The current hart must implement the H extension with a guest interrupt file,
    /// the caller must be permitted to access VS-mode CSRs, `hstatus.VGEIN` must
    /// select an implemented guest interrupt file, and `reg_id` must select an
    /// implemented register for the current XLEN. Otherwise, the indirect CSR access
    /// may raise an illegal- or virtual-instruction exception.
    #[inline]
    pub(super) unsafe fn write_ind(reg_id: usize, value: usize) {
        interrupt::free(|| unsafe {
            let previous = vsiselect::read();
            vsiselect::write(Vsiselect::from_bits(reg_id));
            vsireg::write(Vsireg::from_bits(value));
            vsiselect::write(previous);
        })
    }
}
