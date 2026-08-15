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
