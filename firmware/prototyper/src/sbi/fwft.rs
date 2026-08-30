use rustsbi::SbiRet;
use sbi_spec::fwft::feature_type;

use crate::riscv::csr::CSR_MENVCFG;
use crate::sbi::early_trap::{TrapInfo, csr_read_allow, csr_write_allow};

/// Misaligned load/store exception delegation mask from the RISC-V privileged
/// architecture (exception cause codes 4 and 6).
const MIS_DELEG: usize = (1 << 4) | (1 << 6);

/// `menvcfg` bit fields defined by the corresponding RISC-V extensions.
const ENVCFG_LPE: usize = 1 << 0; // Landing pad (Zicfilp)
const ENVCFG_DTE: usize = 1 << 1; // Double trap (Smdbltrp)
const ENVCFG_ADUE: usize = 1 << 5; // PTE A/D hardware updating (SVADU)
const ENVCFG_SSE: usize = 1 << 8; // Shadow stack (Zicfiss)
const ENVCFG_PMM_SHIFT: usize = 9; // Pointer masking tag length (Smnpm)
const ENVCFG_PMM: usize = 0b11 << ENVCFG_PMM_SHIFT;

/// Implementation of SBI Firmware Features (FWFT) extension.
///
/// - `MISALIGNED_EXC_DELEG` is supported when the hart implements the
///   supervisor mode (`misa.S`); setting it toggles the misaligned load and
///   store bits of `medeleg`, so the S-mode trap handler (rather than the
///   M-mode emulator) receives misaligned accesses.
/// - `LANDING_PAD`, `SHADOW_STACK`, `DOUBLE_TRAP`, `PTE_AD_HW_UPDATING` and
///   `POINTER_MASKING_PMLEN` are backed by the corresponding bits of the
///   `menvcfg` CSR (Zicfilp / Zicfiss / Smdbltrp / SVADU / Smnpm). A feature
///   is reported as supported only if the requested field can be written and
///   read back, which requires the underlying hardware extension.
pub(crate) struct SbiFwft;

impl SbiFwft {
    /// Returns whether the hart implements supervisor mode.
    fn has_s_mode() -> bool {
        riscv::register::misa::read().has_extension('S')
    }

    /// Reads the current misaligned delegation state from `medeleg`.
    fn misaligned_delegated() -> bool {
        (riscv::register::medeleg::read().bits() & MIS_DELEG) != 0
    }

    /// Sets or clears the misaligned delegation bits in `medeleg`.
    fn set_misaligned_delegation(value: usize) -> bool {
        let current = riscv::register::medeleg::read().bits();
        let next = match value {
            0 => current & !MIS_DELEG,
            1 => current | MIS_DELEG,
            _ => return false,
        };
        // Safety: writing `medeleg` from M-mode is a plain CSR store; the
        // value is derived from the current register contents.
        unsafe {
            riscv::register::medeleg::write(riscv::register::medeleg::Medeleg::from_bits(next));
        }
        true
    }

    /// Reads the `menvcfg` CSR when it is implemented by the hart.
    fn menvcfg_read() -> Option<usize> {
        let mut trap = TrapInfo::default();
        let value = unsafe { csr_read_allow::<CSR_MENVCFG>(&mut trap) };
        (trap.mcause == usize::MAX).then_some(value)
    }

    /// Writes the `menvcfg` CSR when it is implemented by the hart.
    fn menvcfg_write(value: usize) -> bool {
        let mut trap = TrapInfo::default();
        unsafe { csr_write_allow::<CSR_MENVCFG>(&mut trap, value) };
        trap.mcause == usize::MAX
    }

    /// Returns the `menvcfg` bit backing a FWFT feature, if any.
    fn menvcfg_bit(feature_id: usize) -> Option<usize> {
        match feature_id {
            feature_type::LANDING_PAD => Some(ENVCFG_LPE),
            feature_type::SHADOW_STACK => Some(ENVCFG_SSE),
            feature_type::DOUBLE_TRAP => Some(ENVCFG_DTE),
            feature_type::PTE_AD_HW_UPDATING => Some(ENVCFG_ADUE),
            _ => None,
        }
    }

    /// Sets or clears a `menvcfg` bit for a FWFT feature.
    ///
    /// The new value is written and read back. A mismatch means the requested
    /// state is not supported by the underlying hardware extension.
    fn set_menvcfg_bit(bit: usize, value: usize) -> SbiRet {
        if value > 1 {
            return SbiRet::invalid_param();
        }
        let Some(current) = Self::menvcfg_read() else {
            return SbiRet::not_supported();
        };
        let next = if value == 1 {
            current | bit
        } else {
            current & !bit
        };
        if !Self::menvcfg_write(next) {
            return SbiRet::not_supported();
        }
        let Some(read_back) = Self::menvcfg_read() else {
            return SbiRet::not_supported();
        };
        if (read_back & bit) != (next & bit) {
            return SbiRet::not_supported();
        }
        SbiRet::success(0)
    }

    /// Sets the pointer masking tag length (`PMM` field of `menvcfg`).
    ///
    /// The new `PMM` value is written and read back. A mismatch means the
    /// requested tag length is not supported by the `Smnpm` extension.
    fn set_pmm(value: usize) -> SbiRet {
        if value > 3 {
            return SbiRet::invalid_param();
        }
        let Some(current) = Self::menvcfg_read() else {
            return SbiRet::not_supported();
        };
        let next = (current & !ENVCFG_PMM) | (value << ENVCFG_PMM_SHIFT);
        if !Self::menvcfg_write(next) {
            return SbiRet::not_supported();
        }
        let Some(read_back) = Self::menvcfg_read() else {
            return SbiRet::not_supported();
        };
        if (read_back & ENVCFG_PMM) != (next & ENVCFG_PMM) {
            return SbiRet::not_supported();
        }
        SbiRet::success(0)
    }

    /// Returns whether the hardware implements the given `menvcfg` bits by
    /// writing them and reading them back. The original value is restored
    /// before returning.
    fn menvcfg_bits_supported(mask: usize) -> bool {
        let Some(current) = Self::menvcfg_read() else {
            return false;
        };
        if !Self::menvcfg_write(current | mask) {
            return false;
        }
        let Some(probed) = Self::menvcfg_read() else {
            let _ = Self::menvcfg_write(current);
            return false;
        };
        let _ = Self::menvcfg_write(current);
        (probed & mask) != 0
    }
}

impl rustsbi::Fwft for SbiFwft {
    fn set(&self, feature_id: u32, value: usize, flags: usize) -> SbiRet {
        // The LOCK flag is not supported: locked features can never be
        // modified again, which would prevent firmware reconfiguration.
        if flags != 0 {
            return SbiRet::invalid_param();
        }
        match feature_id as usize {
            feature_type::MISALIGNED_EXC_DELEG => {
                if !Self::has_s_mode() {
                    return SbiRet::not_supported();
                }
                if Self::set_misaligned_delegation(value) {
                    SbiRet::success(0)
                } else {
                    SbiRet::invalid_param()
                }
            }
            feature_type::POINTER_MASKING_PMLEN => Self::set_pmm(value),
            _ => match Self::menvcfg_bit(feature_id as usize) {
                Some(bit) => Self::set_menvcfg_bit(bit, value),
                None => SbiRet::not_supported(),
            },
        }
    }

    fn get(&self, feature_id: u32) -> SbiRet {
        match feature_id as usize {
            feature_type::MISALIGNED_EXC_DELEG => {
                if !Self::has_s_mode() {
                    return SbiRet::not_supported();
                }
                SbiRet::success(Self::misaligned_delegated() as usize)
            }
            feature_type::POINTER_MASKING_PMLEN => {
                if !Self::menvcfg_bits_supported(ENVCFG_PMM) {
                    return SbiRet::not_supported();
                }
                match Self::menvcfg_read() {
                    Some(value) => SbiRet::success((value & ENVCFG_PMM) >> ENVCFG_PMM_SHIFT),
                    None => SbiRet::not_supported(),
                }
            }
            _ => match Self::menvcfg_bit(feature_id as usize) {
                Some(bit) => {
                    if !Self::menvcfg_bits_supported(bit) {
                        return SbiRet::not_supported();
                    }
                    match Self::menvcfg_read() {
                        Some(value) => SbiRet::success(((value & bit) != 0) as usize),
                        None => SbiRet::not_supported(),
                    }
                }
                None => SbiRet::not_supported(),
            },
        }
    }
}
