//! Decoding and emulation of the retained read-only time CSR case.

/// Architectural time CSR named by a decoded read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TimeCsr {
    /// The complete time value on RV64, or its low 32 bits on RV32.
    Time,
    /// The high 32 bits of time, defined only on RV32.
    TimeHigh,
}

/// A validated read-only CSRRS instruction targeting an architectural time CSR.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DecodedTimeRead {
    /// Integer register that receives the emulated value.
    pub(super) destination_register: usize,
    /// Exact architectural CSR selected by the instruction.
    pub(super) csr: TimeCsr,
}

pub(super) fn decode_time_read(instruction: usize) -> Option<DecodedTimeRead> {
    const OPCODE_SYSTEM: usize = 0x73;
    const FUNCT3_CSRRS: usize = 0b010;
    const CSR_TIME: usize = 0xc01;
    const CSR_TIMEH: usize = 0xc81;

    let instruction = u32::try_from(instruction).ok()? as usize;
    if instruction & 0x7f != OPCODE_SYSTEM
        || (instruction >> 12) & 0b111 != FUNCT3_CSRRS
        || (instruction >> 15) & 0b1_1111 != 0
    {
        return None;
    }
    let destination_register = (instruction >> 7) & 0b1_1111;
    let csr = match (instruction >> 20) & 0xfff {
        CSR_TIME => TimeCsr::Time,
        CSR_TIMEH => TimeCsr::TimeHigh,
        _ => return None,
    };
    Some(DecodedTimeRead {
        destination_register,
        csr,
    })
}
