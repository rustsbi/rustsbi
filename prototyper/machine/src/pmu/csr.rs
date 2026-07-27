//! Closed CSR-immediate dispatch for Zicntr and Zihpm counters.

use crate::trap::probe::{ExpectedResult, probe_csr, swap_csr};

use crate::pmu::hart::CounterError;

pub(in crate::pmu) fn low_value(result: LowResult) -> Result<usize, CounterError> {
    match result {
        LowResult::Value(value) => Ok(value),
        LowResult::Illegal | LowResult::Failure => Err(CounterError::MechanismFailure),
    }
}

#[derive(Clone, Copy)]
pub(in crate::pmu) enum LowResult {
    Value(usize),
    Illegal,
    Failure,
}

pub(in crate::pmu) fn expected_read<const CSR: u16>() -> LowResult {
    // SAFETY: every instantiation is selected by the closed matches below and
    // names only a counter, event-selector, or counter-inhibit CSR.
    match unsafe { probe_csr::<CSR>() } {
        ExpectedResult::Value(value) => LowResult::Value(value),
        ExpectedResult::Fault(fault) if fault.cause == 2 => LowResult::Illegal,
        ExpectedResult::Fault(_) | ExpectedResult::Busy | ExpectedResult::Unavailable => {
            LowResult::Failure
        }
    }
}

pub(in crate::pmu) fn expected_swap<const CSR: u16>(value: usize) -> LowResult {
    // SAFETY: every instantiation is selected by the closed matches below.
    // Counter and selector values are architecturally WARL; required effects
    // are checked by the owning typed operation before success is returned.
    match unsafe { swap_csr::<CSR>(value) } {
        ExpectedResult::Value(previous) => LowResult::Value(previous),
        ExpectedResult::Fault(fault) if fault.cause == 2 => LowResult::Illegal,
        ExpectedResult::Fault(_) | ExpectedResult::Busy | ExpectedResult::Unavailable => {
            LowResult::Failure
        }
    }
}

macro_rules! counter_match {
    ($offset:expr, $operation:ident $(, $argument:expr)?) => {
        match $offset {
            0 => $operation::<0xb00>($($argument)?),
            2 => $operation::<0xb02>($($argument)?),
            3 => $operation::<0xb03>($($argument)?),
            4 => $operation::<0xb04>($($argument)?),
            5 => $operation::<0xb05>($($argument)?),
            6 => $operation::<0xb06>($($argument)?),
            7 => $operation::<0xb07>($($argument)?),
            8 => $operation::<0xb08>($($argument)?),
            9 => $operation::<0xb09>($($argument)?),
            10 => $operation::<0xb0a>($($argument)?),
            11 => $operation::<0xb0b>($($argument)?),
            12 => $operation::<0xb0c>($($argument)?),
            13 => $operation::<0xb0d>($($argument)?),
            14 => $operation::<0xb0e>($($argument)?),
            15 => $operation::<0xb0f>($($argument)?),
            16 => $operation::<0xb10>($($argument)?),
            17 => $operation::<0xb11>($($argument)?),
            18 => $operation::<0xb12>($($argument)?),
            19 => $operation::<0xb13>($($argument)?),
            20 => $operation::<0xb14>($($argument)?),
            21 => $operation::<0xb15>($($argument)?),
            22 => $operation::<0xb16>($($argument)?),
            23 => $operation::<0xb17>($($argument)?),
            24 => $operation::<0xb18>($($argument)?),
            25 => $operation::<0xb19>($($argument)?),
            26 => $operation::<0xb1a>($($argument)?),
            27 => $operation::<0xb1b>($($argument)?),
            28 => $operation::<0xb1c>($($argument)?),
            29 => $operation::<0xb1d>($($argument)?),
            30 => $operation::<0xb1e>($($argument)?),
            31 => $operation::<0xb1f>($($argument)?),
            _ => LowResult::Illegal,
        }
    };
}

pub(in crate::pmu) fn read_counter_low(offset: u8) -> LowResult {
    counter_match!(offset, expected_read)
}

pub(in crate::pmu) fn write_counter_low(offset: u8, value: usize) -> LowResult {
    counter_match!(offset, expected_swap, value)
}

#[cfg(target_pointer_width = "32")]
macro_rules! counter_high_match {
    ($offset:expr, $operation:ident $(, $argument:expr)?) => {
        match $offset {
            0 => $operation::<0xb80>($($argument)?),
            2 => $operation::<0xb82>($($argument)?),
            3 => $operation::<0xb83>($($argument)?),
            4 => $operation::<0xb84>($($argument)?),
            5 => $operation::<0xb85>($($argument)?),
            6 => $operation::<0xb86>($($argument)?),
            7 => $operation::<0xb87>($($argument)?),
            8 => $operation::<0xb88>($($argument)?),
            9 => $operation::<0xb89>($($argument)?),
            10 => $operation::<0xb8a>($($argument)?),
            11 => $operation::<0xb8b>($($argument)?),
            12 => $operation::<0xb8c>($($argument)?),
            13 => $operation::<0xb8d>($($argument)?),
            14 => $operation::<0xb8e>($($argument)?),
            15 => $operation::<0xb8f>($($argument)?),
            16 => $operation::<0xb90>($($argument)?),
            17 => $operation::<0xb91>($($argument)?),
            18 => $operation::<0xb92>($($argument)?),
            19 => $operation::<0xb93>($($argument)?),
            20 => $operation::<0xb94>($($argument)?),
            21 => $operation::<0xb95>($($argument)?),
            22 => $operation::<0xb96>($($argument)?),
            23 => $operation::<0xb97>($($argument)?),
            24 => $operation::<0xb98>($($argument)?),
            25 => $operation::<0xb99>($($argument)?),
            26 => $operation::<0xb9a>($($argument)?),
            27 => $operation::<0xb9b>($($argument)?),
            28 => $operation::<0xb9c>($($argument)?),
            29 => $operation::<0xb9d>($($argument)?),
            30 => $operation::<0xb9e>($($argument)?),
            31 => $operation::<0xb9f>($($argument)?),
            _ => LowResult::Illegal,
        }
    };
}

#[cfg(target_pointer_width = "32")]
pub(in crate::pmu) fn read_counter_high(offset: u8) -> LowResult {
    counter_high_match!(offset, expected_read)
}

#[cfg(target_pointer_width = "32")]
pub(in crate::pmu) fn write_counter_high(offset: u8, value: usize) -> LowResult {
    counter_high_match!(offset, expected_swap, value)
}

macro_rules! event_match {
    ($offset:expr, $operation:ident $(, $argument:expr)?) => {
        match $offset {
            3 => $operation::<0x323>($($argument)?),
            4 => $operation::<0x324>($($argument)?),
            5 => $operation::<0x325>($($argument)?),
            6 => $operation::<0x326>($($argument)?),
            7 => $operation::<0x327>($($argument)?),
            8 => $operation::<0x328>($($argument)?),
            9 => $operation::<0x329>($($argument)?),
            10 => $operation::<0x32a>($($argument)?),
            11 => $operation::<0x32b>($($argument)?),
            12 => $operation::<0x32c>($($argument)?),
            13 => $operation::<0x32d>($($argument)?),
            14 => $operation::<0x32e>($($argument)?),
            15 => $operation::<0x32f>($($argument)?),
            16 => $operation::<0x330>($($argument)?),
            17 => $operation::<0x331>($($argument)?),
            18 => $operation::<0x332>($($argument)?),
            19 => $operation::<0x333>($($argument)?),
            20 => $operation::<0x334>($($argument)?),
            21 => $operation::<0x335>($($argument)?),
            22 => $operation::<0x336>($($argument)?),
            23 => $operation::<0x337>($($argument)?),
            24 => $operation::<0x338>($($argument)?),
            25 => $operation::<0x339>($($argument)?),
            26 => $operation::<0x33a>($($argument)?),
            27 => $operation::<0x33b>($($argument)?),
            28 => $operation::<0x33c>($($argument)?),
            29 => $operation::<0x33d>($($argument)?),
            30 => $operation::<0x33e>($($argument)?),
            31 => $operation::<0x33f>($($argument)?),
            _ => LowResult::Illegal,
        }
    };
}

pub(in crate::pmu) fn write_event_low(offset: u8, value: usize) -> LowResult {
    event_match!(offset, expected_swap, value)
}

pub(in crate::pmu) fn read_event_low(offset: u8) -> LowResult {
    event_match!(offset, expected_read)
}

#[cfg(target_pointer_width = "32")]
macro_rules! event_high_match {
    ($offset:expr, $operation:ident $(, $argument:expr)?) => {
        match $offset {
            3 => $operation::<0x723>($($argument)?),
            4 => $operation::<0x724>($($argument)?),
            5 => $operation::<0x725>($($argument)?),
            6 => $operation::<0x726>($($argument)?),
            7 => $operation::<0x727>($($argument)?),
            8 => $operation::<0x728>($($argument)?),
            9 => $operation::<0x729>($($argument)?),
            10 => $operation::<0x72a>($($argument)?),
            11 => $operation::<0x72b>($($argument)?),
            12 => $operation::<0x72c>($($argument)?),
            13 => $operation::<0x72d>($($argument)?),
            14 => $operation::<0x72e>($($argument)?),
            15 => $operation::<0x72f>($($argument)?),
            16 => $operation::<0x730>($($argument)?),
            17 => $operation::<0x731>($($argument)?),
            18 => $operation::<0x732>($($argument)?),
            19 => $operation::<0x733>($($argument)?),
            20 => $operation::<0x734>($($argument)?),
            21 => $operation::<0x735>($($argument)?),
            22 => $operation::<0x736>($($argument)?),
            23 => $operation::<0x737>($($argument)?),
            24 => $operation::<0x738>($($argument)?),
            25 => $operation::<0x739>($($argument)?),
            26 => $operation::<0x73a>($($argument)?),
            27 => $operation::<0x73b>($($argument)?),
            28 => $operation::<0x73c>($($argument)?),
            29 => $operation::<0x73d>($($argument)?),
            30 => $operation::<0x73e>($($argument)?),
            31 => $operation::<0x73f>($($argument)?),
            _ => LowResult::Illegal,
        }
    };
}

#[cfg(target_pointer_width = "32")]
pub(in crate::pmu) fn write_event_high(offset: u8, value: usize) -> LowResult {
    event_high_match!(offset, expected_swap, value)
}

#[cfg(target_pointer_width = "32")]
pub(in crate::pmu) fn read_event_high(offset: u8) -> LowResult {
    event_high_match!(offset, expected_read)
}
