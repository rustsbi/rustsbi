//! Instruction fetch and data access under the original S/U context.
//! `MPRV` is active only around one byte access, never around Rust code.

use csr::{
    machine,
    values::trap::{CauseValue, Exception, TrapCause},
};

use super::{Error, Fault, decode::ValueKind, frame::TrapInfo};

macro_rules! access_byte {
    ($instruction:literal, $causes:expr, $address:expr, $data:expr, $status:expr) => {{
        let access = || -> Result<usize, Error> {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            let mut data: usize = $data;
            let cause: usize;
            let tval: usize;
            // SAFETY:
            // 1. access_status restricts effective privilege to the original S/U context.
            // 2. A nested trap sets MPP=Machine, so entry can use its stack under MPRV.
            // 3. Entry clears MPRV before Rust and preserves the saved mstatus operand.
            // 4. Success and recovery both restore mstatus before returning to Rust.
            unsafe {
                core::arch::asm!(
                    "csrrci {saved_status}, {status_csr}, 8", // Clear mstatus.MIE.
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
                    crate::recovery::fault_site!(),
                    status_csr = const machine::Status::NUMBER,
                    causes = const $causes,
                    data = inout(reg) data,
                    address = in(reg) $address,
                    requested_status = in(reg) $status,
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
                let TrapCause::Exception(cause) = CauseValue::from(cause).cause() else {
                    unreachable!("recovery accepts synchronous memory faults only")
                };
                Err(Error::Fault(Fault { cause, address: tval.into() }))
            }
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = ($causes, $address, $data, $status);
            unavailable()
        }
        };
        access()
    }};
}

#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
fn unavailable() -> ! {
    unimplemented!("lower-mode access requires RISC-V")
}

fn status_for_return(privilege: machine::TargetPrivilege) -> machine::Status<csr::Write> {
    let mut status = machine::Status::<csr::Write>::default();
    status.set::<machine::MPP>(privilege);
    status
}

fn access_status(info: &TrapInfo, instruction: bool) -> Result<usize, Error> {
    let endianness = match info.origin() {
        machine::Privilege::User => info.status.get::<machine::UBE>(),
        machine::Privilege::Supervisor => {
            #[cfg(target_pointer_width = "32")]
            {
                info.status_high.get::<machine::SBEH>()
            }
            #[cfg(target_pointer_width = "64")]
            {
                info.status.get::<machine::SBE>()
            }
        }
        _ => return Err(Error::Unsupported),
    };
    // Instructions are always little-endian. Data emulation currently assembles
    // bytes in little-endian order, so big-endian data accesses must be forwarded.
    if info.virtualized() || (!instruction && endianness == machine::Endianness::Big) {
        return Err(Error::Unsupported);
    }
    let privilege = match info.origin() {
        machine::Privilege::User => machine::TargetPrivilege::User,
        machine::Privilege::Supervisor => machine::TargetPrivilege::Supervisor,
        _ => unreachable!("origin checked above"),
    };
    let mut status = status_for_return(privilege);
    status.set::<machine::SUM>(info.status.get::<machine::SUM>());
    status.set::<machine::MXR>(if instruction {
        machine::ExecutableReadAccess::Enabled
    } else {
        info.status.get::<machine::MXR>()
    });
    status.set::<machine::MPRV>(machine::AccessPrivilege::Previous);
    Ok(status.into())
}

/// Fetches up to four instruction bytes under the original S/U context.
// Byte loads avoid introducing an alignment fault while inspecting the fault PC.
pub(super) fn fetch(info: &TrapInfo) -> Result<(u32, usize), Error> {
    let status = access_status(info, true)?;
    let byte = |offset: usize| -> Result<u32, Error> {
        let address = info.epc.checked_add(offset).ok_or(Error::Fault(Fault {
            cause: Exception::InstructionAccess,
            address: info.epc.into(),
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
    let low = byte(0)? | (byte(1)? << 8);
    if low & 3 != 3 {
        return Ok((low, 2));
    }
    Ok((low | (byte(2)? << 16) | (byte(3)? << 24), 4))
}

fn check_width(address: usize, kind: ValueKind) -> Result<(), Error> {
    if kind.width() > size_of::<usize>() || address.checked_add(kind.width() - 1).is_none() {
        Err(Error::Unsupported)
    } else {
        Ok(())
    }
}

pub(super) fn read(info: &TrapInfo, kind: ValueKind) -> Result<usize, Error> {
    check_width(info.tval, kind)?;
    let status = access_status(info, false)?;
    let mut value = 0;
    for offset in 0..kind.width() {
        let byte = access_byte!(
            "lbu {data}, 0({address})",
            (1 << 5) | (1 << 13),
            info.tval + offset,
            0usize,
            status
        )?;
        value |= byte << (8 * offset);
    }
    Ok(value)
}

/// Writes a misaligned value under the original S/U context.
///
/// A fault may leave earlier bytes written; writes are not rolled back.
/// See <https://docs.riscv.org/reference/isa/priv/machine.html#pmp>.
// Only misaligned exceptions reach here; access faults are forwarded directly.
pub(super) fn write(info: &TrapInfo, value: usize, kind: ValueKind) -> Result<(), Error> {
    check_width(info.tval, kind)?;
    let status = access_status(info, false)?;
    for offset in 0..kind.width() {
        access_byte!(
            "sb {data}, 0({address})",
            (1 << 7) | (1 << 15),
            info.tval + offset,
            (value >> (8 * offset)) & 0xff,
            status
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(privilege: machine::TargetPrivilege) -> TrapInfo {
        TrapInfo {
            epc: 0x1000,
            status: status_for_return(privilege).into(),
            #[cfg(target_pointer_width = "32")]
            status_high: machine::StatusHigh::from_bits(0),
            #[cfg(target_pointer_width = "64")]
            status_high: 0,
            has_hypervisor: false,
            cause: 0,
            tval: 0x2001,
            tval2: 0,
            instruction: 0,
        }
    }

    #[test]
    fn big_endian_data_is_forwarded_before_any_memory_access() {
        for privilege in [
            machine::TargetPrivilege::User,
            machine::TargetPrivilege::Supervisor,
        ] {
            let mut info = context(privilege);
            let mut status = status_for_return(privilege);
            match privilege {
                machine::TargetPrivilege::User => {
                    status.set::<machine::UBE>(machine::Endianness::Big);
                }
                machine::TargetPrivilege::Supervisor => {
                    #[cfg(target_pointer_width = "32")]
                    {
                        info.status_high = machine::StatusHigh::from_bits(1 << 4);
                    }
                    #[cfg(target_pointer_width = "64")]
                    status.set::<machine::SBE>(machine::Endianness::Big);
                }
                _ => unreachable!(),
            }
            info.status = status.into();
            assert_eq!(read(&info, ValueKind::Unsigned32), Err(Error::Unsupported));
            assert_eq!(
                write(&info, 0x1234_5678, ValueKind::Unsigned32),
                Err(Error::Unsupported)
            );
            // Instruction bytes remain little-endian under either data byte order.
            assert!(access_status(&info, true).is_ok());
        }
    }
}
