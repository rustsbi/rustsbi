//! Pure instruction decoding for trap emulation.
//!
//! Decoding produces a complete description of what the trapped instruction
//! wants; width and signedness are fused into [`ValueKind`] so that
//! meaningless combinations cannot be constructed.

use riscv_decode::Instruction;

use super::Error;

/// How the value transferred by an emulated access is interpreted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueKind {
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
    Signed64,
}

impl ValueKind {
    /// The access width in bytes.
    pub fn width(self) -> usize {
        match self {
            ValueKind::Signed8 | ValueKind::Unsigned8 => 1,
            ValueKind::Signed16 | ValueKind::Unsigned16 => 2,
            ValueKind::Signed32 | ValueKind::Unsigned32 => 4,
            ValueKind::Signed64 => 8,
        }
    }

    /// Interpret raw little-endian bytes (already composed into a word) as a
    /// register value, sign- or zero-extending per the kind.
    pub(crate) fn extend(self, raw: usize) -> usize {
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
pub(crate) struct LoadOp {
    /// Destination register for the loaded value.
    pub rd: u8,
    /// Interpretation of the transferred value.
    pub kind: ValueKind,
}

/// A decoded misaligned store.
pub(crate) struct StoreOp {
    /// Source register holding the value to store.
    pub rs2: u8,
    /// Interpretation of the transferred value.
    pub kind: ValueKind,
}

/// A decoded CSR read instruction (`csrrs rd, csr, x0`).
pub(crate) struct CsrReadOp {
    /// The CSR number being read.
    pub csr: u16,
    /// Destination register for the read value.
    pub rd: u8,
}

// TODO(compressed): support RVC misaligned accesses (c.ld, c.flwsp, Zcb,
// Zcd) and the RV32/RV64 encoding overlap (c.ld@rv64 == c.flw@rv32).
// Reference table: OpenSBI lib/sbi/sbi_trap_ldst.c. Until then these
// instructions are refused and redirected via Err(UnsupportedInstruction).
// Floating-point loads and stores (flw/fld/fsw/fsd) are refused as well:
// their write-back targets live in unsaved floating-point registers.

/// Decode a trapped instruction as a misaligned load.
pub(crate) fn decode_load(raw: u32) -> Result<LoadOp, Error> {
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
        Ok(Instruction::Lwu(i)) => LoadOp {
            rd: i.rd() as u8,
            kind: ValueKind::Unsigned32,
        },
        Ok(Instruction::Ld(i)) => LoadOp {
            rd: i.rd() as u8,
            kind: ValueKind::Signed64,
        },
        _ => return Err(Error::UnsupportedInstruction),
    };
    Ok(op)
}

/// Decode a trapped instruction as a misaligned store.
pub(crate) fn decode_store(raw: u32) -> Result<StoreOp, Error> {
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
        Ok(Instruction::Sd(i)) => StoreOp {
            rs2: i.rs2() as u8,
            kind: ValueKind::Signed64,
        },
        _ => return Err(Error::UnsupportedInstruction),
    };
    Ok(op)
}

/// Decode a trapped instruction as a CSR read.
pub(crate) fn decode_csr_read(raw: u32) -> Result<CsrReadOp, Error> {
    // Pure CSRRS reads require rs1 = x0.
    if raw & 0x000f_f07f != 0x2073 {
        return Err(Error::UnsupportedInstruction);
    }
    let op = match riscv_decode::decode(raw) {
        Ok(Instruction::Csrrs(i)) => CsrReadOp {
            csr: i.csr() as u16,
            rd: i.rd() as u8,
        },
        _ => return Err(Error::UnsupportedInstruction),
    };
    Ok(op)
}
