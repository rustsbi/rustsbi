//! Complete guarded PMP programming and snapshot operations.

use crate::csr::{Misa, PmpAddress, PmpConfig, Readable, Writable};
use crate::pmp::{self, AddressMode, Configuration, EncodedEntry, Error, Permission};

const LOCK: u8 = 1 << 7;

pub(super) fn install(plan: &[EncodedEntry]) -> Result<(), Error> {
    riscv::interrupt::machine::free(|| {
        let old = snapshot_inner(plan.len())?;
        for (index, config) in old.iter().enumerate().take(plan.len()) {
            if config.locked {
                return Err(Error::LockedEntry(index));
            }
        }
        // A locked following TOR entry also locks the preceding address field.
        match read_config(plan.len()) {
            Ok(config)
                if config & LOCK != 0
                    && (config & pmp::ADDRESS_MODE_MASK) >> pmp::ADDRESS_MODE_SHIFT
                        == AddressMode::Tor as u8 =>
            {
                return Err(Error::LockedEntry(plan.len()));
            }
            Ok(_) | Err(crate::trap::Error::UnsupportedInstruction) => {}
            Err(error) => return Err(error.into()),
        }

        if let Err(error) = install_unlocked(plan) {
            // No requested locks have been committed yet. Restore both the
            // addresses and the mode-dependent encodings before returning.
            if restore(&old[..plan.len()]).is_err() {
                fail_stop();
            }
            synchronize();
            return Err(error);
        }

        // A committed lock cannot be undone until reset. Any failure during
        // this phase must stop this hart instead of returning a partial plan.
        for (index, entry) in plan.iter().enumerate() {
            if entry.config & LOCK != 0 && write_config(index, entry.config).is_err() {
                fail_stop();
            }
        }
        synchronize();
        Ok(())
    })
}

fn synchronize() {
    // PMP permissions may be cached even when satp currently selects Bare.
    // Bare-only harts may omit SFENCE.VMA, so try the fixed guarded operation.
    // https://docs.riscv.org/reference/isa/priv/machine.html#_physical_memory_protection_and_paging
    match crate::trap::sfence_vma_guarded() {
        Ok(()) | Err(crate::trap::Error::UnsupportedInstruction) => {}
        Err(_) => fail_stop(),
    }
    if Misa::read()
        .expect("misa is required in M-mode")
        .has_extension('H')
    {
        // Flush cached PMP checks in both G-stage and VS-stage translations,
        // independently of whether policy firmware exposes the H extension.
        crate::instructions::fence::hfence_gvma_all();
    }
}

fn install_unlocked(plan: &[EncodedEntry]) -> Result<(), Error> {
    // Disable matching before changing any address. No new locks are set
    // until all fields have been written and checked on this hart.
    for index in 0..plan.len() {
        write_config(index, 0)?;
    }
    let mut masks = [0; pmp::MAX_ENTRIES];
    let mut expected_addresses = [0; pmp::MAX_ENTRIES];
    for (index, mask) in masks.iter_mut().enumerate().take(plan.len()) {
        // Readable, hardwired-zero PMP CSRs do not establish capacity.
        // Probe writable address bits while all affected matches are OFF.
        write_address(index, usize::MAX)?;
        *mask = read_address(index)?;
        if *mask == 0 {
            return Err(Error::AddressRejected {
                index,
                expected: usize::MAX,
                observed: *mask,
            });
        }
    }
    for (index, entry) in plan.iter().enumerate() {
        let mask = masks[index];
        let napot = entry.config & pmp::ADDRESS_MODE_MASK
            == (AddressMode::Napot as u8) << pmp::ADDRESS_MODE_SHIFT;
        let napot_low = (1usize << mask.trailing_zeros()) - 1;
        let expected = if entry.saturate {
            entry.address & (mask | if napot { napot_low } else { 0 })
        } else {
            entry.address
        };
        expected_addresses[index] = expected;
        write_address(index, entry.address)?;
        let observed = read_address(index)?;
        if observed != expected & mask {
            return Err(Error::AddressRejected {
                index,
                expected: expected & mask,
                observed,
            });
        }
        if !napot && observed != expected {
            return Err(Error::AddressRejected {
                index,
                expected,
                observed,
            });
        }
    }
    for (index, entry) in plan.iter().enumerate() {
        let config = entry.config & !LOCK;
        write_config(index, config)?;
        // The architectural read encoding of low address bits depends on
        // NAPOT mode, so verify those fields only after selecting that mode.
        let observed = read_address(index)?;
        let expected = expected_addresses[index];
        if observed != expected {
            return Err(Error::AddressRejected {
                index,
                expected,
                observed,
            });
        }
    }
    Ok(())
}

fn restore(old: &[Configuration]) -> Result<(), Error> {
    // Keep every affected match OFF until all old address fields are back.
    // Clearing a configuration also checks that no unexpected lock was set.
    for index in 0..old.len() {
        write_config(index, 0)?;
    }
    for (index, configuration) in old.iter().enumerate() {
        write_address(index, configuration.address)?;
    }
    for (index, configuration) in old.iter().enumerate() {
        let config =
            configuration.permission as u8 | (configuration.mode as u8) << pmp::ADDRESS_MODE_SHIFT;
        write_config(index, config)?;
    }
    // WARL readback of the low address bits depends on the restored mode.
    // Checking while OFF would falsely reject old NAPOT encodings.
    for (index, configuration) in old.iter().enumerate() {
        let observed = read_address(index)?;
        if observed != configuration.address {
            return Err(Error::AddressRejected {
                index,
                expected: configuration.address,
                observed,
            });
        }
    }
    Ok(())
}

#[cold]
#[allow(unsafe_code)]
fn fail_stop() -> ! {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    // SAFETY:
    // 1. Installation retains M-mode with machine interrupts disabled.
    // 2. The stack-independent fail-stop vector never returns to this call chain.
    unsafe {
        core::arch::asm!(
            "j {fail_stop}",
            fail_stop = sym crate::boot::fail_stop,
            options(noreturn),
        );
    }
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    panic!("PMP installation cannot recover on a non-RISC-V target");
}

pub(super) fn snapshot(count: usize) -> Result<[Configuration; pmp::MAX_ENTRIES], Error> {
    riscv::interrupt::machine::free(|| snapshot_inner(count))
}

fn snapshot_inner(count: usize) -> Result<[Configuration; pmp::MAX_ENTRIES], Error> {
    let mut configurations = [Configuration {
        mode: AddressMode::Off,
        permission: Permission::None,
        locked: false,
        address: 0,
    }; pmp::MAX_ENTRIES];
    for (index, configuration) in configurations.iter_mut().enumerate().take(count) {
        let config = read_config(index)?;
        *configuration = Configuration {
            mode: match (config & pmp::ADDRESS_MODE_MASK) >> pmp::ADDRESS_MODE_SHIFT {
                0 => AddressMode::Off,
                1 => AddressMode::Tor,
                2 => AddressMode::Na4,
                _ => AddressMode::Napot,
            },
            permission: match config & 7 {
                0 => Permission::None,
                1 => Permission::Read,
                2 => Permission::Write,
                3 => Permission::ReadWrite,
                4 => Permission::Execute,
                5 => Permission::ReadExecute,
                6 => Permission::WriteExecute,
                _ => Permission::ReadWriteExecute,
            },
            locked: config & LOCK != 0,
            address: read_address(index)?,
        };
    }
    Ok(configurations)
}

fn read_address(index: usize) -> Result<usize, crate::trap::Error> {
    seq_macro::seq!(N in 0..16 {
        match index {
            #(N => PmpAddress::<N>::read(),)*
            _ => Err(crate::trap::Error::UnsupportedInstruction),
        }
    })
}

fn write_address(index: usize, address: usize) -> Result<(), crate::trap::Error> {
    seq_macro::seq!(N in 0..16 {
        match index {
            #(N => PmpAddress::<N>::write(address),)*
            _ => Err(crate::trap::Error::UnsupportedInstruction),
        }
    })
}

fn read_config(index: usize) -> Result<u8, crate::trap::Error> {
    let bank = read_config_bank(index / size_of::<usize>())?;
    Ok((bank >> ((index % size_of::<usize>()) * 8)) as u8)
}

fn write_config(index: usize, config: u8) -> Result<(), Error> {
    let bank = index / size_of::<usize>();
    let shift = (index % size_of::<usize>()) * 8;
    let old = read_config_bank(bank)?;
    write_config_bank(
        bank,
        (old & !(0xff << shift)) | (usize::from(config) << shift),
    )?;
    let observed = read_config(index)?;
    if observed != config {
        return Err(Error::ConfigurationRejected {
            index,
            expected: config,
            observed,
        });
    }
    Ok(())
}

fn read_config_bank(bank: usize) -> Result<usize, crate::trap::Error> {
    match bank {
        0 => PmpConfig::<0>::read(),
        #[cfg(target_pointer_width = "32")]
        1 => PmpConfig::<1>::read(),
        #[cfg(target_pointer_width = "32")]
        2 => PmpConfig::<2>::read(),
        #[cfg(target_pointer_width = "32")]
        3 => PmpConfig::<3>::read(),
        #[cfg(target_pointer_width = "32")]
        4 => PmpConfig::<4>::read(),
        #[cfg(target_pointer_width = "64")]
        1 => PmpConfig::<2>::read(),
        #[cfg(target_pointer_width = "64")]
        2 => PmpConfig::<4>::read(),
        _ => Err(crate::trap::Error::UnsupportedInstruction),
    }
}

fn write_config_bank(bank: usize, value: usize) -> Result<(), crate::trap::Error> {
    match bank {
        0 => PmpConfig::<0>::write(value),
        #[cfg(target_pointer_width = "32")]
        1 => PmpConfig::<1>::write(value),
        #[cfg(target_pointer_width = "32")]
        2 => PmpConfig::<2>::write(value),
        #[cfg(target_pointer_width = "32")]
        3 => PmpConfig::<3>::write(value),
        #[cfg(target_pointer_width = "64")]
        1 => PmpConfig::<2>::write(value),
        _ => Err(crate::trap::Error::UnsupportedInstruction),
    }
}
