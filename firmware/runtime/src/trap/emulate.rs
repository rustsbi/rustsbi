//! Instruction emulation for non-virtualized S/U traps.
//!
//! The saved PC advances only after the emulated operation succeeds.

use csr::{
    Read,
    standard::isa::{
        privileged::{machine::Mcounteren, supervisor::Scounteren},
        unprivileged::Time,
        values::{counter::Counter, status::Privilege, xlen::Xlen},
    },
};

use super::{
    Error, decode,
    frame::{TrapFrame, TrapInfo},
    memory,
};
use crate::{hart, timer::now};

pub(super) fn emulate_load(frame: &mut TrapFrame, info: &TrapInfo) -> Result<(), Error> {
    let xlen = info.lower_xlen().ok_or(Error::Unsupported)?;
    let (raw, len) = memory::fetch(info, xlen)?;
    let op = decode::decode_load(raw, xlen).ok_or(Error::Unsupported)?;
    let value = memory::read(info, op.kind, xlen)?;
    frame.general.write(op.rd, op.kind.extend(value));
    frame.advance_pc(len, xlen);
    Ok(())
}

pub(super) fn emulate_store(frame: &mut TrapFrame, info: &TrapInfo) -> Result<(), Error> {
    let xlen = info.lower_xlen().ok_or(Error::Unsupported)?;
    let (raw, len) = memory::fetch(info, xlen)?;
    let op = decode::decode_store(raw, xlen).ok_or(Error::Unsupported)?;
    memory::write(info, frame.general.read(op.rs2), op.kind, xlen)?;
    frame.advance_pc(len, xlen);
    Ok(())
}

pub(super) fn emulate_time_read(frame: &mut TrapFrame, info: &TrapInfo) -> Result<(), Error> {
    let xlen = info.lower_xlen().ok_or(Error::Unsupported)?;
    // mtval may contain instruction bits; otherwise fetch under saved privilege.
    let bits = info.tval as u32;
    let (raw, len) = if bits & 3 == 3 {
        (bits, 4)
    } else {
        memory::fetch(info, xlen)?
    };
    let op = decode::decode_time_read(raw, xlen).ok_or(Error::Unsupported)?;
    // The originating privilege must have time access through each implemented counter gate.
    let privilege = info.origin().map_err(|_| Error::Unsupported)?;
    let machine_counters = Mcounteren::read();
    if !machine_counters.contains(Counter::Time)
        || (privilege == Privilege::User
            && hart::supports_privilege(Privilege::Supervisor)
            && !Scounteren::read().contains(Counter::Time))
    {
        return Err(Error::Unsupported);
    }
    let Some(ticks) = now().map_err(Error::Runtime)? else {
        return Err(Error::Unsupported);
    };
    let value = match xlen {
        Xlen::X32 => {
            let word = if op.csr == Time::HIGH_NUMBER {
                (ticks >> 32) as u32
            } else {
                ticks as u32
            };
            // Reduced-XLEN register results fill MXLEN by sign extension.
            word as i32 as usize
        }
        Xlen::X64 => ticks as usize,
    };
    frame.general.write(op.rd, value);
    frame.advance_pc(len, xlen);
    Ok(())
}
