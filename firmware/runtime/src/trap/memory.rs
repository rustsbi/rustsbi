//! Instruction fetches and misaligned data accesses at the interrupted S/U privilege.

use riscv_csr_spec::standard::isa::{
    privileged::machine::Mstatus,
    values::{address::Address, trap::Exception, xlen::Xlen},
};

use super::{Error, Fault, access, decode::ValueKind, frame::TrapInfo};

fn byte_address(base: usize, offset: usize, xlen: Xlen) -> Option<usize> {
    match xlen {
        Xlen::X32 => Some((base as u32).wrapping_add(offset as u32) as i32 as usize),
        Xlen::X64 => base.checked_add(offset),
    }
}

macro_rules! access_byte {
    ($instruction:literal, $causes:expr, $address:expr, $data:expr, $status:expr) => {{
        let mut data: usize = $data;
        let address = $address;
        let status = $status;
        let cause: usize;
        let tval: usize;
        // SAFETY:
        // 1. access::status restricts effective privilege to the original S/U context.
        // 2. A nested trap sets MPP=Machine, so entry can use its stack under MPRV.
        // 3. Entry clears MPRV before Rust and preserves the saved mstatus operand.
        // 4. Success and recovery both restore mstatus before returning to Rust.
        unsafe {
            core::arch::asm!(
                "csrrci {saved_status}, {status_csr}, 8",
                "not {access_status}, {access_mask}",
                "and {access_status}, {saved_status}, {access_status}",
                "or {access_status}, {access_status}, {requested_status}",
                "li t2, 0",
                "li t3, 0",
                ".option push",
                ".option norvc",
                "csrw {status_csr}, {access_status}",
                "2:",
                $instruction,
                "3:",
                "csrw {status_csr}, {saved_status}",
                ".option pop",
                csr::recovery_record!(),
                status_csr = const Mstatus::NUMBER,
                causes = const $causes,
                data = inout(reg) data,
                address = in(reg) address,
                requested_status = in(reg) status,
                // Replace MIE, MPP, MPRV, SUM, and MXR; preserve other fields.
                access_mask = in(reg) (1usize << 3) | (3 << 11) | (7 << 17),
                saved_status = out(reg) _,
                access_status = out(reg) _,
                out("t2") cause,
                out("t3") tval,
            );
        }
        if cause == 0 {
            Ok(data)
        } else {
            Err(Error::Fault(Fault {
                cause: Exception::from_code(cause),
                address: Address::from_bits(tval),
            }))
        }
    }};
}

/// Fetches a compressed instruction or the first 32 bits of a longer instruction.
///
/// Reads individual bytes with `MXR` set so execute-only pages can supply them.
/// A load access or page fault is reported as the corresponding instruction fault.
pub(super) fn fetch(info: &TrapInfo, xlen: Xlen) -> Result<(u32, usize), Error> {
    let status = access::status(info, true)?;
    let byte_fn = |offset: usize| -> Result<u32, Error> {
        let address = byte_address(info.epc, offset, xlen).ok_or(Error::Fault(Fault {
            cause: Exception::InstructionAccess,
            address: Address::from_bits(info.epc),
        }))?;
        access_byte!(
            "lbu {data}, 0({address})",
            (1 << 5) | (1 << 13),
            address,
            0usize,
            status
        )
        .map(|byte| byte as u32)
        .map_err(|error| match error {
            Error::Fault(mut fault) => {
                fault.cause = match fault.cause {
                    Exception::LoadAccess => Exception::InstructionAccess,
                    Exception::LoadPage => Exception::InstructionPage,
                    other => other,
                };
                Error::Fault(fault)
            }
            other => other,
        })
    };
    let low = byte_fn(0)? | (byte_fn(1)? << 8);
    if low & 3 != 3 {
        return Ok((low, 2));
    }
    Ok((low | (byte_fn(2)? << 16) | (byte_fn(3)? << 24), 4))
}

pub(super) fn read(info: &TrapInfo, kind: ValueKind, xlen: Xlen) -> Result<usize, Error> {
    let status = access::data_status(info, kind, xlen)?;
    let mut value = 0;
    for offset in 0..kind.width() {
        let byte = access_byte!(
            "lbu {data}, 0({address})",
            (1 << 5) | (1 << 13),
            byte_address(info.tval, offset, xlen).ok_or(Error::Unsupported)?,
            0usize,
            status
        )?;
        value |= byte << (8 * offset);
    }
    Ok(value)
}

/// Writes a misaligned value under the original S/U context.
///
/// A fault after the first byte leaves earlier byte stores visible.
pub(super) fn write(
    info: &TrapInfo,
    value: usize,
    kind: ValueKind,
    xlen: Xlen,
) -> Result<(), Error> {
    let status = access::data_status(info, kind, xlen)?;
    for offset in 0..kind.width() {
        access_byte!(
            "sb {data}, 0({address})",
            (1 << 7) | (1 << 15),
            byte_address(info.tval, offset, xlen).ok_or(Error::Unsupported)?,
            (value >> (8 * offset)) & 0xff,
            status
        )?;
    }
    Ok(())
}
