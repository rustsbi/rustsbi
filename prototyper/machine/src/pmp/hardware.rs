//! RISC-V PMP CSR transport for the semantic policy compiler.

use super::hardware_core::{PmpRegisters, install, probe_and_disable};
use crate::pmp::entry::machine_image;
use crate::pmp::policy::compile_machine_policy;
use crate::pmp::state::{PmpError, Region};
use crate::trap::probe::{ExpectedResult, probe_csr, swap_csr};

struct MachineRegisters;

/// Configures this hart before any lower-privilege context can become live.
pub(crate) fn configure_current_hart(
    machine_ranges: &[Region],
    configuration: &crate::pmp::Configuration,
    trusted_without_pmp: bool,
) -> Result<(), PmpError> {
    let disabled = probe_and_disable(MachineRegisters)?;
    let image = compile_machine_policy(
        machine_image()?,
        machine_ranges,
        configuration,
        disabled.capability,
        trusted_without_pmp,
    )?;
    let _verified = install(disabled, image)?;
    Ok(())
}

fn expected_value(result: ExpectedResult) -> Result<Option<usize>, PmpError> {
    match result {
        ExpectedResult::Value(value) => Ok(Some(value)),
        ExpectedResult::Fault(fault) if fault.cause == 2 => Ok(None),
        ExpectedResult::Fault(_) => Err(PmpError::UnexpectedFault),
        ExpectedResult::Busy | ExpectedResult::Unavailable => Err(PmpError::HardwareUnavailable),
    }
}

fn read_fixed<const CSR: u16>() -> Result<Option<usize>, PmpError> {
    // SAFETY: every instantiation is selected from the fixed PMP/mseccfg lists
    // below. The numeric CSR never crosses this private driver boundary.
    expected_value(unsafe { probe_csr::<CSR>() })
}

fn swap_fixed<const CSR: u16>(value: usize) -> Result<Option<usize>, PmpError> {
    // SAFETY: every instantiation is selected from the fixed lists below;
    // callers validate and read back the complete WARL value.
    expected_value(unsafe { swap_csr::<CSR>(value) })
}

macro_rules! dispatch_read {
    ($index:expr; $($slot:literal => $csr:literal),+ $(,)?) => {
        match $index {
            $($slot => read_fixed::<$csr>(),)+
            _ => Ok(None),
        }
    };
}

macro_rules! dispatch_swap {
    ($index:expr, $value:expr; $($slot:literal => $csr:literal),+ $(,)?) => {
        match $index {
            $($slot => swap_fixed::<$csr>($value),)+
            _ => Ok(None),
        }
    };
}

macro_rules! pmpaddr_dispatch {
    ($operation:ident, $index:expr $(, $value:expr)?) => {
        $operation!($index $(, $value)?;
            0 => 0x3b0, 1 => 0x3b1, 2 => 0x3b2, 3 => 0x3b3,
            4 => 0x3b4, 5 => 0x3b5, 6 => 0x3b6, 7 => 0x3b7,
            8 => 0x3b8, 9 => 0x3b9, 10 => 0x3ba, 11 => 0x3bb,
            12 => 0x3bc, 13 => 0x3bd, 14 => 0x3be, 15 => 0x3bf,
            16 => 0x3c0, 17 => 0x3c1, 18 => 0x3c2, 19 => 0x3c3,
            20 => 0x3c4, 21 => 0x3c5, 22 => 0x3c6, 23 => 0x3c7,
            24 => 0x3c8, 25 => 0x3c9, 26 => 0x3ca, 27 => 0x3cb,
            28 => 0x3cc, 29 => 0x3cd, 30 => 0x3ce, 31 => 0x3cf,
            32 => 0x3d0, 33 => 0x3d1, 34 => 0x3d2, 35 => 0x3d3,
            36 => 0x3d4, 37 => 0x3d5, 38 => 0x3d6, 39 => 0x3d7,
            40 => 0x3d8, 41 => 0x3d9, 42 => 0x3da, 43 => 0x3db,
            44 => 0x3dc, 45 => 0x3dd, 46 => 0x3de, 47 => 0x3df,
            48 => 0x3e0, 49 => 0x3e1, 50 => 0x3e2, 51 => 0x3e3,
            52 => 0x3e4, 53 => 0x3e5, 54 => 0x3e6, 55 => 0x3e7,
            56 => 0x3e8, 57 => 0x3e9, 58 => 0x3ea, 59 => 0x3eb,
            60 => 0x3ec, 61 => 0x3ed, 62 => 0x3ee, 63 => 0x3ef,
        )
    };
}

impl PmpRegisters for MachineRegisters {
    fn read_security_config(&mut self) -> Result<Option<usize>, PmpError> {
        read_fixed::<0x747>()
    }

    fn read_config(&mut self, word: usize) -> Result<Option<usize>, PmpError> {
        #[cfg(target_pointer_width = "32")]
        {
            dispatch_read!(word;
                0 => 0x3a0, 1 => 0x3a1, 2 => 0x3a2, 3 => 0x3a3,
                4 => 0x3a4, 5 => 0x3a5, 6 => 0x3a6, 7 => 0x3a7,
                8 => 0x3a8, 9 => 0x3a9, 10 => 0x3aa, 11 => 0x3ab,
                12 => 0x3ac, 13 => 0x3ad, 14 => 0x3ae, 15 => 0x3af,
            )
        }
        #[cfg(target_pointer_width = "64")]
        {
            dispatch_read!(word;
                0 => 0x3a0, 1 => 0x3a2, 2 => 0x3a4, 3 => 0x3a6,
                4 => 0x3a8, 5 => 0x3aa, 6 => 0x3ac, 7 => 0x3ae,
            )
        }
    }

    fn swap_config(&mut self, word: usize, value: usize) -> Result<Option<usize>, PmpError> {
        #[cfg(target_pointer_width = "32")]
        {
            dispatch_swap!(word, value;
                0 => 0x3a0, 1 => 0x3a1, 2 => 0x3a2, 3 => 0x3a3,
                4 => 0x3a4, 5 => 0x3a5, 6 => 0x3a6, 7 => 0x3a7,
                8 => 0x3a8, 9 => 0x3a9, 10 => 0x3aa, 11 => 0x3ab,
                12 => 0x3ac, 13 => 0x3ad, 14 => 0x3ae, 15 => 0x3af,
            )
        }
        #[cfg(target_pointer_width = "64")]
        {
            dispatch_swap!(word, value;
                0 => 0x3a0, 1 => 0x3a2, 2 => 0x3a4, 3 => 0x3a6,
                4 => 0x3a8, 5 => 0x3aa, 6 => 0x3ac, 7 => 0x3ae,
            )
        }
    }

    fn read_address(&mut self, index: usize) -> Result<Option<usize>, PmpError> {
        pmpaddr_dispatch!(dispatch_read, index)
    }

    fn swap_address(&mut self, index: usize, value: usize) -> Result<Option<usize>, PmpError> {
        pmpaddr_dispatch!(dispatch_swap, index, value)
    }
}
