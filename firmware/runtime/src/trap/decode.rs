//! Pure instruction decoding for trap emulation.
//!
//! [`ValueKind`] combines access width and signedness for register write-back.
//! Unsupported instructions return `None` for dispatch to forward.

use riscv_csr_spec::standard::isa::{unprivileged as user, values::xlen::Xlen};
use riscv_decode::Instruction;

/// Transfer width and signedness of an integer memory access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ValueKind {
    Signed8,
    Unsigned8,
    Signed16,
    Unsigned16,
    Signed32,
    Unsigned32,
    Signed64,
}

impl ValueKind {
    /// Returns the access width in bytes.
    pub(super) fn width(self) -> usize {
        match self {
            ValueKind::Signed8 | ValueKind::Unsigned8 => 1,
            ValueKind::Signed16 | ValueKind::Unsigned16 => 2,
            ValueKind::Signed32 | ValueKind::Unsigned32 => 4,
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

/// A pure `time` read, or `timeh` when the interrupted mode has XLEN=32.
pub(super) struct TimeReadOp {
    pub(super) csr: u16,
    pub(super) rd: u8,
}

/// Decodes a 32-bit instruction encoding for an integer load at the interrupted XLEN.
///
/// Floating-point and compressed instructions are forwarded to the supervisor.
pub(super) fn decode_load(raw: u32, xlen: Xlen) -> Option<LoadOp> {
    if raw & 3 != 3 || (xlen == Xlen::X64 && usize::BITS < 64) {
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
        Ok(Instruction::Lwu(i)) if xlen == Xlen::X64 => LoadOp {
            rd: i.rd() as u8,
            kind: ValueKind::Unsigned32,
        },
        Ok(Instruction::Ld(i)) if xlen == Xlen::X64 => LoadOp {
            rd: i.rd() as u8,
            kind: ValueKind::Signed64,
        },
        _ => return None,
    };
    Some(op)
}

/// Decodes a supported integer store.
pub(super) fn decode_store(raw: u32, xlen: Xlen) -> Option<StoreOp> {
    if raw & 3 != 3 || (xlen == Xlen::X64 && usize::BITS < 64) {
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
        Ok(Instruction::Sd(i)) if xlen == Xlen::X64 => StoreOp {
            rs2: i.rs2() as u8,
            kind: ValueKind::Signed64,
        },
        _ => return None,
    };
    Some(op)
}

/// Recognizes supported time reads without accessing hardware.
pub(super) fn decode_time_read(raw: u32, xlen: Xlen) -> Option<TimeReadOp> {
    let (csr, rd) = match riscv_decode::decode(raw).ok()? {
        Instruction::Csrrs(i) | Instruction::Csrrc(i) if i.rs1() == 0 => (i.csr(), i.rd()),
        Instruction::Csrrsi(i) | Instruction::Csrrci(i) if i.zimm() == 0 => (i.csr(), i.rd()),
        _ => return None,
    };
    let csr = csr as u16;
    (csr == user::Time::NUMBER || (xlen == Xlen::X32 && csr == user::Time::HIGH_NUMBER))
        .then_some(TimeReadOp { csr, rd: rd as u8 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compressed_memory_accesses_are_forwarded() {
        // C.LW, C.LD, C.LWSP, C.LDSP; doubleword encodings overlap RV32 FP loads.
        for raw in [0x4000, 0x6000, 0x4082, 0x6082] {
            assert!(
                decode_load(raw, Xlen::NATIVE).is_none(),
                "compressed load {raw:#x}"
            );
        }
        // C.SW, C.SD, C.SWSP, C.SDSP; doubleword encodings overlap RV32 FP stores.
        for raw in [0xc000, 0xe000, 0xc002, 0xe002] {
            assert!(
                decode_store(raw, Xlen::NATIVE).is_none(),
                "compressed store {raw:#x}"
            );
        }
        let load = decode_load(0x0005_2583, Xlen::NATIVE).unwrap(); // lw a1, 0(a0)
        assert_eq!((load.rd, load.kind), (11, ValueKind::Signed32));
        let store = decode_store(0x00b5_2023, Xlen::NATIVE).unwrap(); // sw a1, 0(a0)
        assert_eq!((store.rs2, store.kind), (11, ValueKind::Unsigned32));
        for raw in [0x0005_6583, 0x0005_3583] {
            // lwu/ld a1, 0(a0)
            assert!(decode_load(raw, Xlen::X32).is_none());
        }
        assert!(decode_store(0x00b5_3023, Xlen::X32).is_none()); // sd a1, 0(a0)
    }
}
