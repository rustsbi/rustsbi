//! Compatibility accessors retained while Prototyper migrates to typed CSRs.

struct Raw<const CSR: u16>;

impl<const CSR: u16> csr::Readable for Raw<CSR> {
    const NUMBER: u16 = CSR;
    type Value = usize;
    fn decode(bits: usize) -> usize {
        bits
    }
}

impl<const CSR: u16> csr::Writeable for Raw<CSR> {
    const NUMBER: u16 = CSR;
    type Value = usize;
    fn encode(value: usize) -> usize {
        value
    }
}

/// Reads a CSR for the existing M-mode feature-discovery callers.
///
/// # Safety
///
/// Establish Runtime entry setup and the selected register's read requirements.
pub unsafe fn read_csr_guarded<const CSR: u16>() -> Result<usize, crate::csr::IllegalInstruction> {
    // SAFETY: Existing callers probe side-effect-free CSRs after Runtime entry setup.
    unsafe { crate::csr::read::<Raw<CSR>>() }
}

/// Writes a CSR owned by the existing M-mode feature-discovery caller.
///
/// # Safety
///
/// Establish Runtime entry setup and the selected register's write requirements.
pub unsafe fn write_csr_guarded<const CSR: u16>(
    value: usize,
) -> Result<(), crate::csr::IllegalInstruction> {
    // SAFETY: Existing callers own the probed counter or feature register during startup.
    unsafe { crate::csr::write::<Raw<CSR>>(value) }
}

/// Swaps a counter value owned by the existing M-mode feature-discovery caller.
///
/// # Safety
///
/// Establish Runtime entry setup and both register access requirements.
pub unsafe fn swap_csr_guarded<const CSR: u16>(
    value: usize,
) -> Result<usize, crate::csr::IllegalInstruction> {
    // SAFETY: Existing callers own the probed counter during startup.
    unsafe { crate::csr::swap::<Raw<CSR>>(value) }
}

/// Reports whether misaligned exceptions are delegated to supervisor mode.
pub fn misaligned_delegated() -> bool {
    riscv::register::medeleg::read().bits() & ((1 << 4) | (1 << 6)) != 0
}

/// Changes only the two misaligned-exception delegation bits.
pub fn set_misaligned_delegation(enabled: bool) {
    let current = riscv::register::medeleg::read().bits();
    let mask = (1 << 4) | (1 << 6);
    let next = if enabled {
        current | mask
    } else {
        current & !mask
    };
    // SAFETY: Existing FWFT callers own this hart's lower-mode delegation policy.
    unsafe { riscv::register::medeleg::write(riscv::register::medeleg::Medeleg::from_bits(next)) };
}
