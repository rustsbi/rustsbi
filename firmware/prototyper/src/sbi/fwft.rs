use rustsbi::SbiRet;
use sbi_spec::fwft::feature_type;

use crate::riscv::csr::CSR_MENVCFG;
use crate::sbi::early_trap::{TrapInfo, csr_read_allow, csr_write_allow};
use crate::sbi::trap_stack::with_current;

// Misaligned load/store exception cause codes 4 and 6 from the RISC-V
// privileged architecture.
const MIS_DELEG: usize = (1 << 4) | (1 << 6);

// `menvcfg` fields defined by the corresponding RISC-V extensions.
const ENVCFG_LPE: usize = 1 << 2; // Landing pad (Zicfilp)
const ENVCFG_SSE: usize = 1 << 3; // Shadow stack (Zicfiss)
const ENVCFG_PMM_SHIFT: usize = 32; // Pointer masking tag length (Smnpm)
const ENVCFG_PMM: usize = 0b11 << ENVCFG_PMM_SHIFT;
const ENVCFG_DTE: usize = 1 << 59; // Double trap (Smdbltrp)
const ENVCFG_ADUE: usize = 1 << 61; // PTE A/D hardware updating (SVADU)

const FWFT_LOCK: usize = 1;
const LAST_STANDARD_FEATURE: u32 = feature_type::POINTER_MASKING_PMLEN as u32;

#[derive(Clone, Copy)]
pub(crate) struct FwftState {
    locked: u8,
    probed: u8,
    supported: u8,
}

impl FwftState {
    pub(crate) const fn new() -> Self {
        Self {
            locked: 0,
            probed: 0,
            supported: 0,
        }
    }

    fn mask(feature_id: u32) -> u8 {
        1 << feature_id
    }

    fn is_locked(&self, feature_id: u32) -> bool {
        self.locked & Self::mask(feature_id) != 0
    }

    fn lock(&mut self, feature_id: u32) {
        self.locked |= Self::mask(feature_id);
    }
}

/// Firmware Features extension backed by `medeleg` and `menvcfg`.
///
/// Misaligned exception delegation requires S-mode. Other features require
/// their Zicfilp, Zicfiss, Smdbltrp, Svadu, or Smnpm `menvcfg` fields to retain
/// the requested value when read back.
pub(crate) struct SbiFwft;

impl SbiFwft {
    pub(crate) fn reset_current() {
        let Some(current) = Self::menvcfg_read() else {
            return;
        };
        let controlled = ENVCFG_LPE | ENVCFG_SSE | ENVCFG_PMM | ENVCFG_DTE | ENVCFG_ADUE;
        let _ = Self::menvcfg_write(current & !controlled);
    }

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
        let encoding = match value {
            0 => 0,
            7 => 0b10,
            16 => 0b11,
            _ => return SbiRet::invalid_param(),
        };
        let Some(current) = Self::menvcfg_read() else {
            return SbiRet::not_supported();
        };
        let next = (current & !ENVCFG_PMM) | (encoding << ENVCFG_PMM_SHIFT);
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

    fn probe_menvcfg_bits(mask: usize) -> bool {
        let Some(current) = Self::menvcfg_read() else {
            return false;
        };
        let test_bits = if current & mask == 0 { mask } else { 0 };
        let test_value = (current & !mask) | test_bits;
        if !Self::menvcfg_write(test_value) {
            return false;
        }
        let Some(probed) = Self::menvcfg_read() else {
            let _ = Self::menvcfg_write(current);
            return false;
        };
        if !Self::menvcfg_write(current) {
            return false;
        }
        let Some(restored) = Self::menvcfg_read() else {
            return false;
        };
        probed & mask == test_bits && restored & mask == current & mask
    }

    fn feature_supported(state: &mut FwftState, feature_id: u32, mask: usize) -> bool {
        let feature_mask = FwftState::mask(feature_id);
        if state.probed & feature_mask == 0 {
            state.probed |= feature_mask;
            if Self::probe_menvcfg_bits(mask) {
                state.supported |= feature_mask;
            }
        }
        state.supported & feature_mask != 0
    }

    fn read_feature(state: &mut FwftState, feature_id: u32) -> Result<usize, SbiRet> {
        if feature_id > LAST_STANDARD_FEATURE {
            return Err(SbiRet::denied());
        }

        match feature_id as usize {
            feature_type::MISALIGNED_EXC_DELEG => {
                if !Self::has_s_mode() {
                    return Err(SbiRet::not_supported());
                }
                Ok(Self::misaligned_delegated() as usize)
            }
            feature_type::POINTER_MASKING_PMLEN => {
                if !Self::feature_supported(state, feature_id, ENVCFG_PMM) {
                    return Err(SbiRet::not_supported());
                }
                let value = Self::menvcfg_read().ok_or_else(SbiRet::not_supported)?;
                match (value & ENVCFG_PMM) >> ENVCFG_PMM_SHIFT {
                    0 => Ok(0),
                    0b10 => Ok(7),
                    0b11 => Ok(16),
                    _ => Err(SbiRet::failed()),
                }
            }
            _ => {
                let bit =
                    Self::menvcfg_bit(feature_id as usize).ok_or_else(SbiRet::not_supported)?;
                if !Self::feature_supported(state, feature_id, bit) {
                    return Err(SbiRet::not_supported());
                }
                let value = Self::menvcfg_read().ok_or_else(SbiRet::not_supported)?;
                Ok(((value & bit) != 0) as usize)
            }
        }
    }

    fn valid_value(feature_id: u32, value: usize) -> bool {
        match feature_id as usize {
            feature_type::MISALIGNED_EXC_DELEG
            | feature_type::LANDING_PAD
            | feature_type::SHADOW_STACK
            | feature_type::DOUBLE_TRAP
            | feature_type::PTE_AD_HW_UPDATING => value <= 1,
            feature_type::POINTER_MASKING_PMLEN => matches!(value, 0 | 7 | 16),
            _ => true,
        }
    }
}

impl rustsbi::Fwft for SbiFwft {
    fn set(&self, feature_id: u32, value: usize, flags: usize) -> SbiRet {
        if flags & !FWFT_LOCK != 0 {
            return SbiRet::invalid_param();
        }
        if feature_id > LAST_STANDARD_FEATURE {
            return SbiRet::denied();
        }
        if !Self::valid_value(feature_id, value) {
            return SbiRet::invalid_param();
        }

        with_current(|local| {
            let state = &mut local.fwft_state;
            let current = match Self::read_feature(state, feature_id) {
                Ok(value) => value,
                Err(error) => return error,
            };

            // SBI v3.0 requires an idempotent set to succeed even after the
            // feature has been locked.
            if current == value {
                if flags & FWFT_LOCK != 0 {
                    state.lock(feature_id);
                }
                return SbiRet::success(0);
            }

            if state.is_locked(feature_id) {
                return SbiRet::denied_locked();
            }

            let ret = match feature_id as usize {
                feature_type::MISALIGNED_EXC_DELEG => {
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
            };
            if ret.is_ok() && flags & FWFT_LOCK != 0 {
                state.lock(feature_id);
            }
            ret
        })
    }

    fn get(&self, feature_id: u32) -> SbiRet {
        if feature_id > LAST_STANDARD_FEATURE {
            return SbiRet::denied();
        }
        with_current(
            |local| match Self::read_feature(&mut local.fwft_state, feature_id) {
                Ok(value) => SbiRet::success(value),
                Err(error) => error,
            },
        )
    }
}
