//! Physical memory protection (PMP) entries on the current hart.
//!
//! The [PMP specification](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#pmp)
//! defines the configuration-byte layout and synchronization requirements.

use csr::standard::isa::privileged::machine;
pub use machine::{PmpEntry, PmpPermission, PmpRange, read_pmpaddr as read_addr};

const fn cfg_location(index: u8) -> (u8, usize) {
    let slot = index % size_of::<usize>() as u8;
    // The first entry of pmpcfgN is 4*N. An RV64 word spans eight entries.
    let cfg_reg = (index - slot) / 4;
    (cfg_reg, slot as usize)
}

/// Writes the encoded address, then the configuration byte for PMP entry `index`.
///
/// Returns `false` for indices outside 0 through 63 without accessing a CSR.
/// A successful instruction can still leave a locked entry unchanged or read
/// back a WARL value different from the request. Use [`read_entry`] to check it.
///
/// # Safety
///
/// 1. Execute in M-mode on a hart implementing the selected CSRs and exclude
///    other writers to the address and its containing configuration word.
/// 2. Keep firmware and lower-mode accesses valid through both writes. A
///    following TOR entry uses this address as its lower bound as well.
/// 3. Synchronize translations before relying on the new permissions.
pub unsafe fn write_entry(index: u8, entry: PmpEntry, addr: usize) -> bool {
    let (cfg_reg, slot) = cfg_location(index);
    let Some(current) = machine::read_pmpcfg(cfg_reg) else {
        return false;
    };
    // SAFETY:
    // 1. The caller owns these CSRs and permits the old configuration to match
    //    the new address between the two writes.
    // 2. The mask preserves every neighbouring entry in the same word.
    unsafe {
        if !machine::write_pmpaddr(index, addr) {
            return false;
        }
        let shift = slot * 8;
        let updated = (current & !(0xffusize << shift)) | ((entry.bits() as usize) << shift);
        machine::write_pmpcfg(cfg_reg, updated)
    }
}

/// Reads the configuration byte and encoded address of a PMP entry.
///
/// The pair is not an atomic snapshot. Indices outside 0 through 63 yield
/// `None`. Missing CSRs at a valid index raise an illegal-instruction exception.
pub fn read_entry(index: u8) -> Option<(PmpEntry, usize)> {
    let (cfg_reg, slot) = cfg_location(index);
    let word = machine::read_pmpcfg(cfg_reg)?;
    let addr = machine::read_pmpaddr(index)?;
    let entry = PmpEntry::from_bits_truncate((word >> (slot * 8)) as u8);
    Some((entry, addr))
}
