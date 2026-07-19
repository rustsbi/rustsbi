//! Per-hart PMP discovery, fail-closed installation, and exact readback.

use super::state::{AddressMode, Capability, Entry, Image, MAX_PMP_ENTRIES, Permissions, PmpError};

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod riscv;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(super) use riscv::configure_current_hart;

const PMP_L: u8 = 1 << 7;
const MSECCFG_STANDARD_INCOMPATIBLE: usize = 0b111;
const CONFIG_BYTES_PER_CSR: usize = core::mem::size_of::<usize>();
const MAX_CONFIG_CSRS: usize = MAX_PMP_ENTRIES / CONFIG_BYTES_PER_CSR;

trait PmpRegisters {
    fn read_security_config(&mut self) -> Result<Option<usize>, PmpError>;
    fn read_config(&mut self, word: usize) -> Result<Option<usize>, PmpError>;
    fn swap_config(&mut self, word: usize, value: usize) -> Result<Option<usize>, PmpError>;
    fn read_address(&mut self, index: usize) -> Result<Option<usize>, PmpError>;
    fn swap_address(&mut self, index: usize, value: usize) -> Result<Option<usize>, PmpError>;
}

struct DisabledPmp<R> {
    registers: R,
    capability: Capability,
}

struct VerifiedPmp<R> {
    _registers: R,
}

/// Configures this hart before any lower-privilege context can become live.
///
/// Discovery first proves that every existing entry is unlocked, then leaves
/// every configuration byte OFF. Installation writes and verifies all deny
/// addresses and deny configurations before enabling the final broad entry.
fn probe_and_disable<R: PmpRegisters>(mut registers: R) -> Result<DisabledPmp<R>, PmpError> {
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

fn install<R: PmpRegisters>(
    mut disabled: DisabledPmp<R>,
    image: Image,
) -> Result<VerifiedPmp<R>, PmpError> {
    let entries = match image {
        Image::TrustedWithoutPmp if disabled.capability.entries == 0 => {
            return Ok(VerifiedPmp {
                _registers: disabled.registers,
            });
        }
        Image::TrustedWithoutPmp => return Err(PmpError::VerificationFailed),
        Image::Protected(entries) => entries,
    };
    validate_image(&entries, disabled.capability)?;

    let grain_address_mask = disabled.capability.granularity / 4 - 1;
    for (index, entry) in entries.iter().enumerate() {
        write_and_verify_address_as(
            &mut disabled.registers,
            index,
            entry.address,
            entry.address & !grain_address_mask,
        )?;
    }

    let broad_index = entries.len() - 1;
    let broad_word = broad_index / CONFIG_BYTES_PER_CSR;
    let broad_shift = (broad_index % CONFIG_BYTES_PER_CSR) * 8;
    let mut deny_configs = [0usize; MAX_CONFIG_CSRS];
    for (index, entry) in entries[..broad_index].iter().enumerate() {
        let word = index / CONFIG_BYTES_PER_CSR;
        let shift = (index % CONFIG_BYTES_PER_CSR) * 8;
        deny_configs[word] |= usize::from(entry.config_byte()) << shift;
    }

    // Every exclusion becomes effective and is read back before the broad S/U
    // remainder can be enabled. A fault at any earlier step only restricts the
    // hart; a fault at the final step occurs after all exclusions are active.
    for (word, config) in deny_configs[..=broad_word].iter().copied().enumerate() {
        write_and_verify_config(&mut disabled.registers, word, config)?;
    }
    let final_broad_word =
        deny_configs[broad_word] | (usize::from(entries[broad_index].config_byte()) << broad_shift);
    write_and_verify_config(&mut disabled.registers, broad_word, final_broad_word)?;

    for (index, entry) in entries.iter().enumerate() {
        if disabled.registers.read_address(index)? != Some(entry.address) {
            return Err(PmpError::VerificationFailed);
        }
    }
    for (word, config) in deny_configs[..=broad_word].iter().copied().enumerate() {
        let expected = if word == broad_word {
            final_broad_word
        } else {
            config
        };
        if disabled.registers.read_config(word)? != Some(expected) {
            return Err(PmpError::VerificationFailed);
        }
    }

    Ok(VerifiedPmp {
        _registers: disabled.registers,
    })
}

fn validate_image(entries: &[Entry], capability: Capability) -> Result<(), PmpError> {
    if entries.is_empty() || entries.len() > capability.entries {
        return Err(PmpError::VerificationFailed);
    }
    let broad = entries.last().ok_or(PmpError::VerificationFailed)?;
    if broad.address != capability.napot_address_mask
        || broad.permissions != Permissions::all()
        || broad.mode != AddressMode::NaturallyAlignedPowerOfTwo
    {
        return Err(PmpError::VerificationFailed);
    }
    for entry in &entries[..entries.len() - 1] {
        if entry.address & !capability.napot_address_mask != 0
            || !matches!(
                entry.mode,
                AddressMode::NaturallyAlignedFourBytes | AddressMode::NaturallyAlignedPowerOfTwo
            )
            || !entry.permissions.is_empty()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pmp::{Region, compile};
    use alloc::vec::Vec;

    const PMP_ADDRESS_MODE_MASK: u8 = 0b11 << 3;

    struct FakeRegisters {
        entries: usize,
        config: [usize; MAX_CONFIG_CSRS],
        address: [usize; MAX_PMP_ENTRIES],
        off_address_mask: usize,
        security_config: Option<usize>,
        config_writes: Vec<(usize, usize)>,
    }

    impl FakeRegisters {
        fn new(entries: usize, grain_bits: u32, address_bits: u32) -> Self {
            let low = (1usize << grain_bits) - 1;
            let address_mask = (1usize << address_bits) - 1;
            Self {
                entries,
                config: [0; MAX_CONFIG_CSRS],
                address: [0; MAX_PMP_ENTRIES],
                off_address_mask: address_mask & !low,
                security_config: Some(0),
                config_writes: Vec::new(),
            }
        }

        fn config_byte(&self, index: usize) -> u8 {
            let word = index / CONFIG_BYTES_PER_CSR;
            let shift = (index % CONFIG_BYTES_PER_CSR) * 8;
            (self.config[word] >> shift) as u8
        }

        fn read_address_value(&self, index: usize) -> usize {
            let mut value = self.address[index] & self.off_address_mask;
            if self.config_byte(index) & PMP_ADDRESS_MODE_MASK
                == AddressMode::NaturallyAlignedPowerOfTwo.bits()
            {
                value |= (1usize << self.off_address_mask.trailing_zeros()) - 1;
            }
            value
        }
    }

    impl PmpRegisters for FakeRegisters {
        fn read_security_config(&mut self) -> Result<Option<usize>, PmpError> {
            Ok(self.security_config)
        }

        fn read_config(&mut self, word: usize) -> Result<Option<usize>, PmpError> {
            Ok((word < self.entries.div_ceil(CONFIG_BYTES_PER_CSR)).then_some(self.config[word]))
        }

        fn swap_config(&mut self, word: usize, value: usize) -> Result<Option<usize>, PmpError> {
            if word >= self.entries.div_ceil(CONFIG_BYTES_PER_CSR) {
                return Ok(None);
            }
            let old = self.config[word];
            self.config[word] = value;
            self.config_writes.push((word, value));
            Ok(Some(old))
        }

        fn read_address(&mut self, index: usize) -> Result<Option<usize>, PmpError> {
            Ok((index < self.entries).then(|| self.read_address_value(index)))
        }

        fn swap_address(&mut self, index: usize, value: usize) -> Result<Option<usize>, PmpError> {
            if index >= self.entries {
                return Ok(None);
            }
            let old = self.read_address_value(index);
            self.address[index] = value;
            Ok(Some(old))
        }
    }

    #[test]
    fn probe_discovers_count_granularity_and_full_napot_mask() {
        let disabled = probe_and_disable(FakeRegisters::new(16, 2, 20)).unwrap();
        assert_eq!(
            disabled.capability,
            Capability::new(16, 16, (1 << 20) - 1).unwrap()
        );
        assert!(disabled.registers.config.iter().all(|value| *value == 0));
        assert!(disabled.registers.address.iter().all(|value| *value == 0));
    }

    #[test]
    fn locked_or_extended_state_is_rejected_before_mutation() {
        let mut locked = FakeRegisters::new(16, 0, 20);
        locked.config[0] = usize::from(PMP_L);
        assert!(matches!(
            probe_and_disable(locked),
            Err(PmpError::LockedState)
        ));

        let mut extended = FakeRegisters::new(16, 0, 20);
        extended.security_config = Some(1);
        assert!(matches!(
            probe_and_disable(extended),
            Err(PmpError::ExtendedState)
        ));
    }

    #[test]
    fn no_pmp_requires_the_explicit_trusted_mode() {
        let disabled = probe_and_disable(FakeRegisters::new(0, 0, 20)).unwrap();
        assert_eq!(disabled.capability, Capability::new(0, 4, 0).unwrap());
        assert_eq!(
            compile(&[], disabled.capability, false),
            Err(PmpError::PmpRequired)
        );
        let image = compile(&[], disabled.capability, true).unwrap();
        assert!(install(disabled, image).is_ok());
    }

    #[test]
    fn broad_allow_is_written_only_after_all_denies() {
        let disabled = probe_and_disable(FakeRegisters::new(16, 0, 32)).unwrap();
        let image = compile(
            &[
                Region::new(0x1000, 0x3000).unwrap(),
                Region::new(0x8000, 0x9000).unwrap(),
            ],
            disabled.capability,
            false,
        )
        .unwrap();
        let broad_index = match &image {
            Image::Protected(entries) => entries.len() - 1,
            Image::TrustedWithoutPmp => unreachable!(),
        };
        let verified = install(disabled, image).unwrap();
        let broad_word = broad_index / CONFIG_BYTES_PER_CSR;
        let writes = &verified._registers.config_writes;
        let final_write = writes.last().copied().unwrap();
        assert_eq!(final_write.0, broad_word);
        assert_ne!(
            (final_write.1 >> ((broad_index % CONFIG_BYTES_PER_CSR) * 8)) as u8
                & Permissions::all().bits(),
            0
        );
        assert!(writes[..writes.len() - 1].iter().all(|(word, value)| {
            *word != broad_word
                || ((*value >> ((broad_index % CONFIG_BYTES_PER_CSR) * 8)) as u8
                    & Permissions::all().bits())
                    == 0
        }));
    }
}
