use rustsbi::SbiRet;
use sbi_spec::fwft::feature_type;

use crate::riscv::csr::CSR_MENVCFG;
use crate::sbi::early_trap::{TrapInfo, csr_read_allow, csr_write_allow};

// Misaligned load/store exception cause codes 4 and 6 from the RISC-V
// privileged architecture.
const MIS_DELEG: usize = (1 << 4) | (1 << 6);

// `menvcfg` fields defined by the corresponding RISC-V extensions.
const ENVCFG_LPE: usize = 1 << 0; // Landing pad (Zicfilp)
const ENVCFG_DTE: usize = 1 << 1; // Double trap (Smdbltrp)
const ENVCFG_ADUE: usize = 1 << 5; // PTE A/D hardware updating (SVADU)
const ENVCFG_SSE: usize = 1 << 8; // Shadow stack (Zicfiss)
const ENVCFG_PMM_SHIFT: usize = 9; // Pointer masking tag length (Smnpm)
const ENVCFG_PMM: usize = 0b11 << ENVCFG_PMM_SHIFT;

/// Firmware Features extension backed by `medeleg` and `menvcfg`.
///
/// Misaligned exception delegation requires S-mode. Other features require
/// their Zicfilp, Zicfiss, Smdbltrp, Svadu, or Smnpm `menvcfg` fields to retain
/// the requested value when read back.
pub(crate) struct SbiFwft;

impl SbiFwft {
    fn has_s_mode() -> bool {
        riscv::register::misa::read().has_extension('S')
    }

    fn misaligned_delegated() -> bool {
        (riscv::register::medeleg::read().bits() & MIS_DELEG) != 0
    }

    fn set_misaligned_delegation(value: usize) -> bool {
        let current = riscv::register::medeleg::read().bits();
        let next = match value {
            0 => current & !MIS_DELEG,
            1 => current | MIS_DELEG,
            _ => return false,
        };
        // SAFETY: the prototyper runs in M-mode, and `next` preserves every
        // `medeleg` bit except the two misaligned exception bits.
        unsafe {
            riscv::register::medeleg::write(riscv::register::medeleg::Medeleg::from_bits(next));
        }
        true
    }

    fn menvcfg_read() -> Option<usize> {
        let mut trap = TrapInfo::default();
        // SAFETY: firmware runs in M-mode, and `trap` remains valid for the call.
        let value = unsafe { csr_read_allow::<CSR_MENVCFG>(&mut trap) };
        (trap.mcause == usize::MAX).then_some(value)
    }

    fn menvcfg_write(value: usize) -> bool {
        let mut trap = TrapInfo::default();
        // SAFETY: firmware runs in M-mode, and `trap` remains valid for the call.
        unsafe { csr_write_allow::<CSR_MENVCFG>(&mut trap, value) };
        trap.mcause == usize::MAX
    }

    fn menvcfg_bit(feature_id: usize) -> Option<usize> {
        match feature_id {
            feature_type::LANDING_PAD => Some(ENVCFG_LPE),
            feature_type::SHADOW_STACK => Some(ENVCFG_SSE),
            feature_type::DOUBLE_TRAP => Some(ENVCFG_DTE),
            feature_type::PTE_AD_HW_UPDATING => Some(ENVCFG_ADUE),
            _ => None,
        }
    }

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
        // WARL fields may ignore writes when the backing extension is absent.
        if (read_back & bit) != (next & bit) {
            return SbiRet::not_supported();
        }
        SbiRet::success(0)
    }

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
        // PMM is WARL and may reject tag lengths unsupported by Smnpm.
        if (read_back & ENVCFG_PMM) != (next & ENVCFG_PMM) {
            return SbiRet::not_supported();
        }
        SbiRet::success(0)
    }

    // Probe whether WARL fields retain set bits, then attempt to restore
    // the original value.
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
