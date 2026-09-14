//! Small, typed wrappers for the machine CSRs used by Runtime.
//!
//! The wrappers keep the unavoidable privileged instructions in Runtime. A
//! platform adapter therefore only has to acknowledge its own device; it
//! does not need to know how Runtime enables or signals a RISC-V interrupt.

/// Reads the current hart's architectural identifier.
#[inline]
pub fn mhartid() -> usize {
    riscv::register::mhartid::read()
}

/// The supervisor timer compare CSR introduced by Sstc.
pub(crate) const STIMECMP: u16 = 0x14d;

/// Probes whether the current hart implements Sstc's `stimecmp` CSR.
///
/// The probe is only used during Runtime trap initialization, after the
/// guarded-fault entry is available; an absent CSR is reported as `false`.
#[inline(never)]
pub(crate) fn has_stimecmp() -> bool {
    crate::trap::read_csr_guarded::<STIMECMP>().is_ok()
}

/// Machine-interrupt enable bits used by the Runtime trap mechanism.
pub mod mie {
    /// Enables machine software interrupts on the current hart.
    #[inline]
    pub fn set_software() {
        // SAFETY: Runtime owns the machine trap state on the current hart.
        unsafe { riscv::register::mie::set_msoft() }
    }

    /// Disables machine software interrupts on the current hart.
    #[inline]
    pub fn clear_software() {
        // SAFETY: Runtime owns the machine trap state on the current hart.
        unsafe { riscv::register::mie::clear_msoft() }
    }

    /// Enables machine timer interrupts on the current hart.
    #[inline]
    pub fn set_timer() {
        // SAFETY: Runtime owns the machine trap state on the current hart.
        unsafe { riscv::register::mie::set_mtimer() }
    }

    /// Disables machine timer interrupts on the current hart.
    #[inline]
    pub fn clear_timer() {
        // SAFETY: Runtime owns the machine trap state on the current hart.
        unsafe { riscv::register::mie::clear_mtimer() }
    }

    /// Enables machine external interrupts on the current hart.
    #[inline]
    pub fn set_external() {
        // SAFETY: Runtime owns the machine trap state on the current hart.
        unsafe { riscv::register::mie::set_mext() }
    }
}

/// Machine-interrupt pending bits used by the Runtime trap mechanism.
pub mod mip {
    /// Signals a supervisor software interrupt to the next-stage supervisor.
    #[inline]
    pub fn set_supervisor_software() {
        // SAFETY: Runtime owns the machine trap state on the current hart.
        unsafe { riscv::register::mip::set_ssoft() }
    }

    /// Signals a supervisor timer interrupt to the next-stage supervisor.
    #[inline]
    pub fn set_supervisor_timer() {
        // SAFETY: Runtime owns the machine trap state on the current hart.
        unsafe { riscv::register::mip::set_stimer() }
    }
}
