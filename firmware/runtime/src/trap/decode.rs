//! Pure instruction decoding for trap emulation.
//!
//! [`ValueKind`] combines access width and signedness for register write-back.
//! Unsupported instructions return `None` for dispatch to forward.

use crate::timer::TimeWord;
use csr::user;
use riscv_decode::Instruction;

/// Width and signedness of an emulated memory access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ValueKind {
    /// 1-byte load, sign-extended.
    Signed8,
    /// 1-byte load/store, zero-extended.
    Unsigned8,
    /// 2-byte load, sign-extended.
    Signed16,
    /// 2-byte load/store, zero-extended.
    Unsigned16,
    /// 4-byte load, sign-extended.
    Signed32,
    /// 4-byte load/store, zero-extended.
    Unsigned32,
    /// 8-byte load/store.
    #[cfg(target_pointer_width = "64")]
    Signed64,
}

impl ValueKind {
    /// Returns the access width in bytes.
    pub(super) fn width(self) -> usize {
        match self {
            ValueKind::Signed8 | ValueKind::Unsigned8 => 1,
            ValueKind::Signed16 | ValueKind::Unsigned16 => 2,
            ValueKind::Signed32 | ValueKind::Unsigned32 => 4,
            #[cfg(target_pointer_width = "64")]
            ValueKind::Signed64 => 8,
        }
    }

    /// Extends an assembled memory word to the destination register width.
    pub(super) fn extend(self, raw: usize) -> usize {
        match self {
            ValueKind::Signed8 => raw as i8 as usize,
            ValueKind::Unsigned8 => raw as u8 as usize,
            ValueKind::Signed16 => raw as i16 as usize,
            ValueKind::Unsigned16 => raw as u16 as usize,
            ValueKind::Signed32 => raw as i32 as usize,
            ValueKind::Unsigned32 => raw as u32 as usize,
            #[cfg(target_pointer_width = "64")]
            ValueKind::Signed64 => raw,
        }
    }
}

/// A decoded misaligned load.
pub(super) struct LoadOp {
    /// Destination register for the loaded value.
    pub(super) rd: u8,
    /// Interpretation of the transferred value.
    pub(super) kind: ValueKind,
}

/// A decoded misaligned store.
pub(super) struct StoreOp {
    /// Source register holding the value to store.
    pub(super) rs2: u8,
    /// Interpretation of the transferred value.
    pub(super) kind: ValueKind,
}

/// A pure `time` or RV32 `timeh` read.
pub(super) struct TimeReadOp {
    pub(super) word: TimeWord,
    /// Destination register for the read value.
    pub(super) rd: u8,
}

// Compressed accesses are forwarded until XLEN-dependent encodings are supported.
// Floating-point accesses are forwarded because entry does not save those registers.
// Encoding reference: https://github.com/riscv-software-src/opensbi/blob/master/lib/sbi/sbi_trap_ldst.c

/// Decodes a supported integer load.
pub(super) fn decode_load(raw: u32) -> Option<LoadOp> {
    if raw & 3 != 3 {
        return None;
    }
    let op = match riscv_decode::decode(raw) {
        Ok(Instruction::Lb(i)) => LoadOp {
            rd: i.rd() as u8,
            kind: ValueKind::Signed8,
        },
        Ok(Instruction::Lbu(i)) => LoadOp {
            rd: i.rd() as u8,
            kind: ValueKind::Unsigned8,
        },
        Ok(Instruction::Lh(i)) => LoadOp {
            rd: i.rd() as u8,
            kind: ValueKind::Signed16,
        },
        Ok(Instruction::Lhu(i)) => LoadOp {
            rd: i.rd() as u8,
            kind: ValueKind::Unsigned16,
        },
        Ok(Instruction::Lw(i)) => LoadOp {
            rd: i.rd() as u8,
            kind: ValueKind::Signed32,
        },
        #[cfg(target_pointer_width = "64")]
        Ok(Instruction::Lwu(i)) => LoadOp {
            rd: i.rd() as u8,
            kind: ValueKind::Unsigned32,
        },
        #[cfg(target_pointer_width = "64")]
        Ok(Instruction::Ld(i)) => LoadOp {
            rd: i.rd() as u8,
            kind: ValueKind::Signed64,
        },
        _ => return None,
    };
    Some(op)
}

/// Decodes a supported integer store.
pub(super) fn decode_store(raw: u32) -> Option<StoreOp> {
    if raw & 3 != 3 {
        return None;
    }
    let op = match riscv_decode::decode(raw) {
        Ok(Instruction::Sb(i)) => StoreOp {
            rs2: i.rs2() as u8,
            kind: ValueKind::Unsigned8,
        },
        Ok(Instruction::Sh(i)) => StoreOp {
            rs2: i.rs2() as u8,
            kind: ValueKind::Unsigned16,
        },
        Ok(Instruction::Sw(i)) => StoreOp {
            rs2: i.rs2() as u8,
            kind: ValueKind::Unsigned32,
        },
        #[cfg(target_pointer_width = "64")]
        Ok(Instruction::Sd(i)) => StoreOp {
            rs2: i.rs2() as u8,
            kind: ValueKind::Signed64,
        },
        _ => return None,
    };
    Some(op)
}

/// Recognizes supported time reads without accessing hardware.
pub(super) fn decode_time_read(raw: u32) -> Option<TimeReadOp> {
    // Pure CSRRS reads require rs1 = x0.
    if raw & 0x000f_f07f != 0x2073 {
        return None;
    }
    let op = match riscv_decode::decode(raw) {
        Ok(Instruction::Csrrs(i)) => {
            let word = match i.csr() as u16 {
                user::Time::NUMBER => TimeWord::Low,
                #[cfg(target_pointer_width = "32")]
                user::TimeHigh::NUMBER => TimeWord::High,
                _ => return None,
            };
            TimeReadOp {
                word,
                rd: i.rd() as u8,
            }
        }
        _ => return None,
    };
    Some(op)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compressed_memory_accesses_are_forwarded() {
        // C.LW, C.LD, C.LWSP, C.LDSP; doubleword encodings overlap RV32 FP loads.
        for raw in [0x4000, 0x6000, 0x4082, 0x6082] {
            assert!(decode_load(raw).is_none(), "compressed load {raw:#x}");
        }
        // C.SW, C.SD, C.SWSP, C.SDSP; doubleword encodings overlap RV32 FP stores.
        for raw in [0xc000, 0xe000, 0xc002, 0xe002] {
            assert!(decode_store(raw).is_none(), "compressed store {raw:#x}");
        }
        let load = decode_load(0x0005_2583).unwrap(); // lw a1, 0(a0)
        assert_eq!((load.rd, load.kind), (11, ValueKind::Signed32));
        let store = decode_store(0x00b5_2023).unwrap(); // sw a1, 0(a0)
        assert_eq!((store.rs2, store.kind), (11, ValueKind::Unsigned32));
    }

    #[test]
    fn time_emulation_accepts_only_supported_pure_reads() {
        // csrrs a0, time, x0
        let raw = 0xc010_2573;
        let op = decode_time_read(raw).unwrap();
        assert_eq!(op.rd, 10);
        assert_eq!(op.word, TimeWord::Low);
        // A non-x0 source attempts a write regardless of the source value.
        assert!(decode_time_read(raw | (1 << 15)).is_none());
        // csrrw a0, time, x0 also writes the CSR.
        assert!(decode_time_read(0xc010_1573).is_none());
        // Pure reads of other CSRs must retain their original exception.
        assert!(decode_time_read(0x7c30_2573).is_none());
        assert!(decode_time_read(0).is_none());
    }

    #[test]
    fn high_time_word_is_available_only_on_rv32() {
        let op = decode_time_read(0xc810_2573);
        #[cfg(target_pointer_width = "32")]
        assert_eq!(op.unwrap().word, TimeWord::High);
        #[cfg(target_pointer_width = "64")]
        assert!(op.is_none());
    }
}
