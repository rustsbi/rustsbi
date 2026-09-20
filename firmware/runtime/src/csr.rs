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

/// State-enable registers used to prepare a supervisor next stage.
pub mod stateen {
    use core::arch::asm;

    const MSTATEEN0: u16 = 0x30c;
    const MSTATEEN1: u16 = 0x30d;
    const MSTATEEN2: u16 = 0x30e;
    const MSTATEEN3: u16 = 0x30f;
    const SSTATEEN0: u16 = 0x10c;
    const SSTATEEN1: u16 = 0x10d;
    const SSTATEEN2: u16 = 0x10e;
    const SSTATEEN3: u16 = 0x10f;
    const HSTATEEN0: u16 = 0x60c;
    const HSTATEEN1: u16 = 0x60d;
    const HSTATEEN2: u16 = 0x60e;
    const HSTATEEN3: u16 = 0x60f;

    const CONTEXT: u64 = 1u64 << 57;
    const IMSIC: u64 = 1u64 << 58;
    const AIA: u64 = 1u64 << 59;
    const SVSLCT: u64 = 1u64 << 60;
    const ENVCFG: u64 = 1u64 << 62;
    const STATEN: u64 = 1u64 << 63;

    /// Configures state access for the supervisor next stage on this hart.
    ///
    /// An implementation without Smstateen needs no configuration. When
    /// Smstateen is present, Runtime also clears every implemented lower-level
    /// state-enable register as required before entering a fresh supervisor.
    /// `aia_enabled` exposes supervisor AIA and IMSIC state.
    pub fn configure_supervisor(aia_enabled: bool) {
        if crate::trap::read_csr_guarded::<MSTATEEN0>().is_err() {
            return;
        }

        let mut stateen0 = STATEN | CONTEXT | ENVCFG;
        if aia_enabled {
            stateen0 |= IMSIC | AIA | SVSLCT;
        }
        // CTR is intentionally omitted until Prototyper independently detects
        // Ssctr; it is not an AIA capability.

        write64::<MSTATEEN0, { MSTATEEN0 + 0x10 }>(stateen0);
        write64::<MSTATEEN1, { MSTATEEN1 + 0x10 }>(STATEN);
        write64::<MSTATEEN2, { MSTATEEN2 + 0x10 }>(STATEN);
        write64::<MSTATEEN3, { MSTATEEN3 + 0x10 }>(STATEN);

        if crate::trap::read_csr_guarded::<SSTATEEN0>().is_ok() {
            write::<SSTATEEN0>(0);
            write::<SSTATEEN1>(0);
            write::<SSTATEEN2>(0);
            write::<SSTATEEN3>(0);
        }
        if crate::trap::read_csr_guarded::<HSTATEEN0>().is_ok() {
            write64::<HSTATEEN0, { HSTATEEN0 + 0x10 }>(0);
            write64::<HSTATEEN1, { HSTATEEN1 + 0x10 }>(0);
            write64::<HSTATEEN2, { HSTATEEN2 + 0x10 }>(0);
            write64::<HSTATEEN3, { HSTATEEN3 + 0x10 }>(0);
        }
    }

    #[inline(always)]
    fn write<const CSR: u16>(value: usize) {
        // SAFETY: configure_supervisor probes the relevant state-enable bank
        // before use, and Smstateen defines each implemented bank completely.
        unsafe {
            asm!("csrw {csr}, {value}", csr = const CSR, value = in(reg) value, options(nomem))
        }
    }

    #[inline(always)]
    fn write64<const CSR: u16, const CSR_HIGH: u16>(value: u64) {
        write::<CSR>(value as usize);
        #[cfg(target_pointer_width = "32")]
        write::<CSR_HIGH>((value >> 32) as usize);
    }
}

/// Machine-interrupt enable bits used by the Runtime trap mechanism.
pub mod mie {
    /// Enables machine software interrupts on the current hart.
    #[inline]
    pub fn set_machine_software() {
        // SAFETY: Runtime owns the machine trap state on the current hart.
        unsafe { riscv::register::mie::set_msoft() }
    }

    /// Enables machine timer interrupts on the current hart.
    #[inline]
    pub fn set_machine_timer() {
        // SAFETY: Runtime owns the machine trap state on the current hart.
        unsafe { riscv::register::mie::set_mtimer() }
    }

    /// Disables machine timer interrupts on the current hart.
    #[inline]
    pub fn clear_machine_timer() {
        // SAFETY: Runtime owns the machine trap state on the current hart.
        unsafe { riscv::register::mie::clear_mtimer() }
    }

    /// Enables machine external interrupts on the current hart.
    #[inline]
    pub fn set_machine_external() {
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

    /// Clears the supervisor timer interrupt pending bit on the current hart.
    #[inline]
    pub fn clear_supervisor_timer() {
        // SAFETY: Runtime owns the machine trap state on the current hart.
        // Writes to STIP are ignored when Sstc drives it.
        unsafe { riscv::register::mip::clear_stimer() }
    }
}
