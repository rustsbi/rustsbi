//! Private optional-CSR instructions and boot configuration.

pub mod thead;
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
pub(crate) use crate::recovery::unavailable;
pub(crate) use crate::recovery::{csrr, csrrw, csrw};

/// Enables supervisor access to seed when the security CSR is available.
pub fn enable_supervisor_seed() {
    if !crate::boot::traps_ready() {
        return;
    }
    if csrr!(csr::machine::SecurityConfiguration).is_ok() {
        // SAFETY: The fixed SSEED bit permits supervisor seed access without
        // changing M-mode. One CSRS preserves concurrent updates to other bits.
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        unsafe {
            core::arch::asm!("csrs {csr}, {mask}",
            csr = const csr::machine::SecurityConfiguration::NUMBER,
            mask = in(reg) 1usize << 9, options(nomem));
        }
    }
}

/// State-enable registers used to prepare a supervisor next stage.
pub mod stateen {
    use core::arch::asm;
    use csr::{hypervisor, machine, supervisor};

    const MSTATEEN0: u16 = machine::StateEnable0::NUMBER;
    const MSTATEEN1: u16 = 0x30d;
    const MSTATEEN2: u16 = 0x30e;
    const MSTATEEN3: u16 = 0x30f;
    const SSTATEEN0: u16 = supervisor::StateEnable0::NUMBER;
    const SSTATEEN1: u16 = 0x10d;
    const SSTATEEN2: u16 = 0x10e;
    const SSTATEEN3: u16 = 0x10f;
    const HSTATEEN0: u16 = hypervisor::StateEnable0::NUMBER;
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
    /// An implementation without Smstateen needs no configuration.
    /// When Smstateen is present, Runtime also clears every implemented
    /// lower-level state-enable register as required before entering a fresh
    /// supervisor.
    /// `aia_enabled` exposes supervisor AIA and IMSIC state.
    pub fn configure_supervisor(aia_enabled: bool) {
        // SAFETY: Runtime entry is installed. State-enable reads have no
        // side effects, and this function runs in the M-mode boot context.
        if super::csrr!(machine::StateEnable0).is_err() {
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

        // SAFETY: Same boot context and side-effect-free bank probe.
        if super::csrr!(supervisor::StateEnable0).is_ok() {
            write::<SSTATEEN0>(0);
            write::<SSTATEEN1>(0);
            write::<SSTATEEN2>(0);
            write::<SSTATEEN3>(0);
        }
        // SAFETY: Same boot context and side-effect-free bank probe.
        if super::csrr!(hypervisor::StateEnable0).is_ok() {
            write64::<HSTATEEN0, { HSTATEEN0 + 0x10 }>(0);
            write64::<HSTATEEN1, { HSTATEEN1 + 0x10 }>(0);
            write64::<HSTATEEN2, { HSTATEEN2 + 0x10 }>(0);
            write64::<HSTATEEN3, { HSTATEEN3 + 0x10 }>(0);
        }
    }

    #[inline(always)]
    fn write<const CSR: u16>(value: usize) {
        // SAFETY: Configure_supervisor probes the relevant state-enable bank
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
