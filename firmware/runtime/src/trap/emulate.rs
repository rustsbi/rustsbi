//! Complete instruction emulation.
//! Only successful operations advance the PC.
use csr::{machine, supervisor, user, values::counter::Counter};

use super::{
    Error, decode,
    frame::{TrapFrame, TrapInfo},
    memory,
};

pub(super) fn emulate_load(frame: &mut TrapFrame, info: &TrapInfo) -> Result<(), Error> {
    let (raw, len) = memory::fetch(info)?;
    let op = decode::decode_load(raw).ok_or(Error::Unsupported)?;
    let value = memory::read(info, op.kind)?;
    frame.general.write(op.rd, op.kind.extend(value));
    frame.epc = frame.epc.wrapping_add(len);
    Ok(())
}

pub(super) fn emulate_store(frame: &mut TrapFrame, info: &TrapInfo) -> Result<(), Error> {
    let (raw, len) = memory::fetch(info)?;
    let op = decode::decode_store(raw).ok_or(Error::Unsupported)?;
    memory::write(info, frame.general.read(op.rs2), op.kind)?;
    frame.epc = frame.epc.wrapping_add(len);
    Ok(())
}

pub(super) fn emulate_time_read(frame: &mut TrapFrame, info: &TrapInfo) -> Result<(), Error> {
    // mtval may contain instruction bits; otherwise fetch under saved privilege.
    let bits = info.tval as u32;
    let (raw, len) = if bits & 3 == 3 {
        (bits, 4)
    } else {
        memory::fetch(info)?
    };
    let op = decode::decode_time_read(raw).ok_or(Error::Unsupported)?;
    // https://docs.riscv.org/reference/isa/priv/supervisor.html#scounteren
    // Emulation must preserve the original mode's counter-access restrictions.
    let machine_counters = machine::CounterEnable::read();
    let supervisor_counters = supervisor::CounterEnable::read();
    if !machine_counters.contains(Counter::Time)
        || (info.origin() == machine::Privilege::User
            && !supervisor_counters.contains(Counter::Time))
    {
        return Err(Error::Unsupported);
    }
    let timer = crate::timer::get().ok_or(Error::Unsupported)?;
    let value = match op.csr {
        user::Time::NUMBER => timer.read_time(),
        #[cfg(target_pointer_width = "32")]
        user::TimeHigh::NUMBER => timer.read_time_high(),
        _ => return Err(Error::Unsupported),
    };
    frame.general.write(op.rd, value.ok_or(Error::Unsupported)?);
    frame.epc = frame.epc.wrapping_add(len);
    Ok(())
}
