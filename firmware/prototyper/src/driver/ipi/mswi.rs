//! ACLINT MSWI: per-hart MSIP at +4.
//!
//! # References
//!
//! - Specification: [RISC-V ACLINT 1.0-rc4](https://github.com/riscvarchive/riscv-aclint/blob/4e570bfd3201f2c09e5afd290b5091526b0f099a/riscv-aclint.adoc) —
//!   "Machine-level Software Interrupt Device (MSWI)": one 32-bit MSIP register
//!   per hart at `+4 * hart`, whose least significant bit carries the pending
//!   state.
//! - Reference implementation: [QEMU ACLINT SWI device](https://github.com/qemu/qemu/blob/v10.2.1/hw/intc/riscv_aclint.c) —
//!   the 4-byte MSIP stride used by the `virt` machine.

use super::{IpiBackend, IpiError, IpiRequest};
use crate::cfg::NUM_HART_MAX;
use crate::platform::HartIndexMap;
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

const MSIP_STRIDE: usize = 4;

#[repr(u32)]
enum IpiState {
    Clear = 0,
    Pending = 1,
}

pub(in crate::driver) struct Mswi {
    registers: MmioRegion,
    hart_indices: HartIndexMap,
}

pub(in crate::driver) fn bind(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
    hart_indices: HartIndexMap,
) -> runtime::Result<Mswi> {
    let hart_count =
        usize::try_from(hart_indices.count()).map_err(|_| runtime::Error::InvalidArgs)?;
    if hart_count == 0 || hart_count > NUM_HART_MAX {
        return Err(runtime::Error::InvalidArgs);
    }
    let window = registers.subrange(0, MSIP_STRIDE * hart_count)?;
    if !window.has_aligned_bounds(MSIP_STRIDE) {
        return Err(runtime::Error::InvalidArgs);
    }
    Ok(Mswi {
        registers: memory.acquire_mmio(window)?,
        hart_indices,
    })
}

impl Mswi {
    fn write(&self, hart_id: usize, value: IpiState) -> Result<(), IpiError> {
        // MSIP registers are addressed by the index this device assigns to the
        // hart, which need not equal the hart ID.
        let index = self.hart_indices.get(hart_id).ok_or(IpiError::Failed)? as usize;
        self.registers
            .write(MSIP_STRIDE * index, value as u32)
            .map_err(|_| IpiError::Failed)
    }
}

impl IpiBackend for Mswi {
    /// Raises the machine-level software interrupt on each selected target with
    /// one MSIP write per hart. [`IpiBackend::send_ipi`] documents the
    /// supervisor-visible effect; the target hart's machine trap turns that MSIP
    /// into SSIP (`sbi::ipi`).
    #[inline(always)]
    fn send_ipi(&self, request: IpiRequest) -> Result<(), IpiError> {
        for hart_id in request.target_hart_ids() {
            self.write(hart_id, IpiState::Pending)?;
        }
        Ok(())
    }

    #[inline(always)]
    fn clear_ipi(&self, hart_id: usize) -> Result<(), IpiError> {
        self.write(hart_id, IpiState::Clear)
    }
}
