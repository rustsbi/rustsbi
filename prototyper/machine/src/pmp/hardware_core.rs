//! Per-hart PMP discovery, fail-closed installation, and exact readback.

use super::state::{AddressMode, Capability, Entry, Image, MAX_PMP_ENTRIES, PmpError};

const PMP_L: u8 = 1 << 7;
const MSECCFG_STANDARD_INCOMPATIBLE: usize = 0b111;
const CONFIG_BYTES_PER_CSR: usize = core::mem::size_of::<usize>();
const MAX_CONFIG_CSRS: usize = MAX_PMP_ENTRIES / CONFIG_BYTES_PER_CSR;

pub(super) trait PmpRegisters {
    fn read_security_config(&mut self) -> Result<Option<usize>, PmpError>;
    fn read_config(&mut self, word: usize) -> Result<Option<usize>, PmpError>;
    fn swap_config(&mut self, word: usize, value: usize) -> Result<Option<usize>, PmpError>;
    fn read_address(&mut self, index: usize) -> Result<Option<usize>, PmpError>;
    fn swap_address(&mut self, index: usize, value: usize) -> Result<Option<usize>, PmpError>;
}

pub(super) struct DisabledPmp<R> {
    pub(super) registers: R,
    pub(super) capability: Capability,
}

pub(super) struct VerifiedPmp<R> {
    _registers: R,
}

/// Configures this hart before any lower-privilege context can become live.
///
/// Discovery first proves that every existing entry is unlocked, then leaves
/// every configuration byte OFF. Installation makes the complete machine deny
/// floor effective before enabling any lower-privilege grant.
pub(super) fn probe_and_disable<R: PmpRegisters>(
    mut registers: R,
) -> Result<DisabledPmp<R>, PmpError> {
    if registers
        .read_security_config()?
        .is_some_and(|value| value & MSECCFG_STANDARD_INCOMPATIBLE != 0)
    {
        return Err(PmpError::ExtendedState);
    }

    let mut config_words = 0;
    for word in 0..MAX_CONFIG_CSRS {
        let Some(config) = registers.read_config(word)? else {
            break;
        };
        if config
            .to_le_bytes()
            .into_iter()
            .any(|byte| byte & PMP_L != 0)
        {
            return Err(PmpError::LockedState);
        }
        config_words += 1;
    }

    // No lower-privilege context is reachable during preparation. Turning all
    // entries OFF before touching addresses therefore cannot create an
    // observable permissive window, and any later failure keeps this hart in M.
    for word in 0..config_words {
        write_and_verify_config(&mut registers, word, 0)?;
    }

    let mut entries = 0;
    let mut off_address_mask = None;
    let mut reached_unimplemented_entry = false;
    for index in 0..MAX_PMP_ENTRIES {
        let supported = registers.swap_address(index, usize::MAX)?.is_some();
        let observed = registers.read_address(index)?.unwrap_or(0);
        if !supported || observed == 0 {
            reached_unimplemented_entry = true;
            continue;
        }
        if reached_unimplemented_entry {
            return Err(PmpError::InconsistentCapability);
        }
        if off_address_mask
            .replace(observed)
            .is_some_and(|mask| mask != observed)
        {
            return Err(PmpError::InconsistentCapability);
        }
        write_and_verify_address(&mut registers, index, 0)?;
        entries += 1;
    }

    if entries == 0 {
        return Ok(DisabledPmp {
            registers,
            capability: Capability::new(0, 4, 0)?,
        });
    }
    if entries > config_words * CONFIG_BYTES_PER_CSR {
        return Err(PmpError::InconsistentCapability);
    }

    let off_address_mask = off_address_mask.ok_or(PmpError::InconsistentCapability)?;
    let grain_bits = off_address_mask.trailing_zeros();
    let granularity_shift = grain_bits
        .checked_add(2)
        .filter(|shift| *shift < usize::BITS)
        .ok_or(PmpError::InvalidCapability)?;
    let granularity = 1usize << granularity_shift;

    if registers.swap_address(0, usize::MAX)?.is_none() {
        return Err(PmpError::InconsistentCapability);
    }
    write_and_verify_config(
        &mut registers,
        0,
        usize::from(AddressMode::NaturallyAlignedPowerOfTwo.bits()),
    )?;
    let napot_address_mask = registers
        .read_address(0)?
        .ok_or(PmpError::InconsistentCapability)?;
    write_and_verify_config(&mut registers, 0, 0)?;
    write_and_verify_address(&mut registers, 0, 0)?;

    let forced_low_mask = napot_address_mask ^ off_address_mask;
    let expected_low_mask = granularity
        .checked_div(4)
        .and_then(|words| words.checked_sub(1))
        .ok_or(PmpError::InvalidCapability)?;
    if forced_low_mask != expected_low_mask
        || off_address_mask | expected_low_mask != napot_address_mask
    {
        return Err(PmpError::InconsistentCapability);
    }

    Ok(DisabledPmp {
        registers,
        capability: Capability::new(entries, granularity, napot_address_mask)?,
    })
}

pub(super) fn install<R: PmpRegisters>(
    mut disabled: DisabledPmp<R>,
    image: Image,
) -> Result<VerifiedPmp<R>, PmpError> {
    let (entries, deny_count) = match image {
        Image::TrustedWithoutPmp if disabled.capability.entries == 0 => {
            return Ok(VerifiedPmp {
                _registers: disabled.registers,
            });
        }
        Image::TrustedWithoutPmp => return Err(PmpError::VerificationFailed),
        Image::Protected {
            entries,
            deny_count,
        } => (entries, deny_count),
    };
    validate_image(&entries, deny_count, disabled.capability)?;

    let grain_address_mask = disabled.capability.granularity / 4 - 1;
    for (index, entry) in entries.iter().enumerate() {
        write_and_verify_address_as(
            &mut disabled.registers,
            index,
            entry.address,
            entry.address & !grain_address_mask,
        )?;
    }

    let mut deny_configs = [0usize; MAX_CONFIG_CSRS];
    let mut final_configs = [0usize; MAX_CONFIG_CSRS];
    for (index, entry) in entries.iter().enumerate() {
        let word = index / CONFIG_BYTES_PER_CSR;
        let shift = (index % CONFIG_BYTES_PER_CSR) * 8;
        let config = usize::from(entry.config_byte()) << shift;
        final_configs[word] |= config;
        if index < deny_count {
            deny_configs[word] |= config;
        }
    }
    let used_words = entries.len().div_ceil(CONFIG_BYTES_PER_CSR);

    // Every machine exclusion becomes effective and is read back before any
    // S/U grant is enabled. A failure therefore leaves the hart in M-mode with
    // an equal or more restrictive policy.
    for (word, config) in deny_configs[..used_words].iter().copied().enumerate() {
        write_and_verify_config(&mut disabled.registers, word, config)?;
    }
    for (word, config) in final_configs[..used_words].iter().copied().enumerate() {
        write_and_verify_config(&mut disabled.registers, word, config)?;
    }

    for (index, entry) in entries.iter().enumerate() {
        if disabled.registers.read_address(index)? != Some(entry.address) {
            return Err(PmpError::VerificationFailed);
        }
    }
    for (word, config) in final_configs[..used_words].iter().copied().enumerate() {
        if disabled.registers.read_config(word)? != Some(config) {
            return Err(PmpError::VerificationFailed);
        }
    }

    Ok(VerifiedPmp {
        _registers: disabled.registers,
    })
}

fn validate_image(
    entries: &[Entry],
    deny_count: usize,
    capability: Capability,
) -> Result<(), PmpError> {
    if entries.is_empty() || entries.len() > capability.entries || deny_count > entries.len() {
        return Err(PmpError::VerificationFailed);
    }
    for (index, entry) in entries.iter().enumerate() {
        if entry.address & !capability.napot_address_mask != 0
            || !matches!(
                entry.mode,
                AddressMode::NaturallyAlignedFourBytes | AddressMode::NaturallyAlignedPowerOfTwo
            )
            || (index < deny_count) != entry.permissions.is_empty()
        {
            return Err(PmpError::VerificationFailed);
        }
    }
    Ok(())
}

fn write_and_verify_config<R: PmpRegisters>(
    registers: &mut R,
    word: usize,
    value: usize,
) -> Result<(), PmpError> {
    if registers.swap_config(word, value)?.is_none() || registers.read_config(word)? != Some(value)
    {
        return Err(PmpError::VerificationFailed);
    }
    Ok(())
}

fn write_and_verify_address<R: PmpRegisters>(
    registers: &mut R,
    index: usize,
    value: usize,
) -> Result<(), PmpError> {
    write_and_verify_address_as(registers, index, value, value)
}

fn write_and_verify_address_as<R: PmpRegisters>(
    registers: &mut R,
    index: usize,
    value: usize,
    expected: usize,
) -> Result<(), PmpError> {
    if registers.swap_address(index, value)?.is_none()
        || registers.read_address(index)? != Some(expected)
    {
        return Err(PmpError::VerificationFailed);
    }
    Ok(())
}
