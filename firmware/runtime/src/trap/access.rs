//! Machine status for memory accesses on behalf of an interrupted S/U context.

use riscv_csr_spec::standard::isa::{
    privileged::machine::Mstatus,
    values::{
        status::{Endianness, Privilege},
        xlen::Xlen,
    },
};

use super::{Error, decode::ValueKind, frame::TrapInfo};

pub(super) fn status(info: &TrapInfo, instruction: bool) -> Result<usize, Error> {
    let privilege = info.origin().map_err(|_| Error::Unsupported)?;
    let endianness = match privilege {
        Privilege::User => info.status.get(Mstatus::UBE),
        Privilege::Supervisor => info.supervisor_endianness(),
        _ => return Err(Error::Unsupported),
    };
    // Instructions are always little-endian. Data emulation assembles bytes
    // in little-endian order, so big-endian data accesses must be forwarded.
    if info.virtualized() || (!instruction && endianness == Endianness::Big) {
        return Err(Error::Unsupported);
    }
    let mut status = Mstatus::zero();
    status.set(Mstatus::MPP, privilege);
    status.set(Mstatus::SUM, info.status.get(Mstatus::SUM));
    status.set(Mstatus::MXR, instruction || info.status.get(Mstatus::MXR));
    status.set(Mstatus::MPRV, true);
    Ok(status.bits())
}

pub(super) fn data_status(info: &TrapInfo, kind: ValueKind, xlen: Xlen) -> Result<usize, Error> {
    if kind.width() > xlen.bytes()
        || (xlen == Xlen::X64 && info.tval.checked_add(kind.width() - 1).is_none())
    {
        return Err(Error::Unsupported);
    }
    status(info, false)
}
