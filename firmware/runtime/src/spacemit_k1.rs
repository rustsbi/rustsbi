//! SpacemiT K1 system-register descriptions.
//!
//! Compatibility reference: the pinned [OpenSBI K1 platform header] defines
//! the SpacemiT K1 addresses used here.
//!
//! [OpenSBI K1 platform header]: https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/platform/generic/include/spacemit/k1.h

use core::mem::size_of;

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
use core::arch::asm;

use serde_device_tree::buildin::{Node, StrSeq};

use crate::Result;
use crate::memory::{DeviceRegisterRange, PhysAddr, PhysAddrRange};

const SPACEMIT_K1_COMPATIBLE: &str = "spacemit,k1";
const K1_RESET_VECTOR_BASES: [PhysAddr; 2] =
    [PhysAddr::new(0xd428_2db0), PhysAddr::new(0xd428_2eb0)];
const K1_CCI_BASE: PhysAddr = PhysAddr::new(0xd850_0000);
const K1_CCI_FIRST_INTERFACE_OFFSET: usize = 0x1000;
const K1_CCI_INTERFACE_STRIDE: usize = 0x1000;
const K1_HARTS_PER_CLUSTER: usize = 4;

#[repr(u16)]
enum K1Csr {
    MachineSetup = 0x7c0,
    MachineL2Setup = 0x7f0,
}

struct MachineSetup(usize);

impl MachineSetup {
    const DATA_CACHE_ENABLE: usize = 1 << 0;
    const INSTRUCTION_CACHE_ENABLE: usize = 1 << 1;
    const BRANCH_PREDICTION_ENABLE: usize = 1 << 4;
    const PREFETCH_ENABLE: usize = 1 << 5;
    const MISALIGNED_ACCESS_ENABLE: usize = 1 << 6;
    const ECC_ENABLE: usize = 1 << 16;

    const fn enabled() -> Self {
        Self(
            Self::DATA_CACHE_ENABLE
                | Self::INSTRUCTION_CACHE_ENABLE
                | Self::BRANCH_PREDICTION_ENABLE
                | Self::PREFETCH_ENABLE
                | Self::MISALIGNED_ACCESS_ENABLE
                | Self::ECC_ENABLE,
        )
    }
}

#[repr(usize)]
enum ResetVectorRegister {
    AddressHigh = 0x04,
}

#[repr(usize)]
enum CciRegister {
    SnoopControl = 0x0000,
    Status = 0x000c,
}

/// K1 system-register ranges authorized by the Platform Description.
#[derive(Clone, Copy)]
pub struct SpacemitK1Registers {
    reset_vectors: [DeviceRegisterRange; 2],
    cci_status: DeviceRegisterRange,
    cci_snoop_controls: [DeviceRegisterRange; 2],
}

impl SpacemitK1Registers {
    /// Returns K1 system registers when the root compatible list identifies K1.
    pub(crate) fn from_root(root: &Node<'_>) -> Result<Option<Self>> {
        let Some(compatible) = root.get_prop("compatible") else {
            return Ok(None);
        };
        if !compatible
            .deserialize::<StrSeq>()
            .iter()
            .any(|value| value == SPACEMIT_K1_COMPATIBLE)
        {
            return Ok(None);
        }

        let reset_vector_span = ResetVectorRegister::AddressHigh as usize + size_of::<u32>();
        let reset_vectors = [
            fixed_register_range(K1_RESET_VECTOR_BASES[0], reset_vector_span)?,
            fixed_register_range(K1_RESET_VECTOR_BASES[1], reset_vector_span)?,
        ];
        let cci_status = fixed_register_range(
            K1_CCI_BASE
                .checked_add(CciRegister::Status as usize)
                .ok_or(crate::Error::Overflow)?,
            size_of::<u32>(),
        )?;
        let cci_snoop_controls = [cci_snoop_control_range(0)?, cci_snoop_control_range(1)?];

        Ok(Some(Self {
            reset_vectors,
            cci_status,
            cci_snoop_controls,
        }))
    }

    /// Returns the two cluster reset-vector register ranges.
    pub const fn reset_vectors(self) -> [DeviceRegisterRange; 2] {
        self.reset_vectors
    }

    /// Returns the CCI status-register range.
    pub const fn cci_status(self) -> DeviceRegisterRange {
        self.cci_status
    }

    /// Returns the two cluster CCI snoop-control ranges.
    pub const fn cci_snoop_controls(self) -> [DeviceRegisterRange; 2] {
        self.cci_snoop_controls
    }

    /// Enables this hart's bit in the K1 cluster L2 setup register.
    pub fn enable_hart_l2(self, hart_id: usize) {
        let cluster_hart = hart_id % K1_HARTS_PER_CLUSTER;
        set_csr::<{ K1Csr::MachineL2Setup as u16 }>(1 << cluster_hart);
    }

    /// Enables the K1 machine-mode cache and prediction features used by firmware.
    pub fn enable_machine_features(self) {
        set_csr::<{ K1Csr::MachineSetup as u16 }>(MachineSetup::enabled().0);
    }
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
#[inline]
fn set_csr<const CSR: u16>(bits: usize) {
    // SAFETY: `SpacemitK1Registers` is created only after the Platform
    // Description identifies a K1. These implementation-defined CSRs affect
    // only machine features of the current hart.
    unsafe {
        asm!(
            "csrs {csr}, {bits}",
            csr = const CSR,
            bits = in(reg) bits,
            options(nomem)
        );
    }
}

#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
fn set_csr<const CSR: u16>(_: usize) {
    let _ = CSR;
    panic!("SpacemiT K1 CSR access requires a RISC-V target");
}

fn cci_snoop_control_range(interface_index: usize) -> Result<DeviceRegisterRange> {
    let offset = K1_CCI_INTERFACE_STRIDE
        .checked_mul(interface_index)
        .and_then(|offset| K1_CCI_FIRST_INTERFACE_OFFSET.checked_add(offset))
        .and_then(|offset| offset.checked_add(CciRegister::SnoopControl as usize))
        .ok_or(crate::Error::Overflow)?;
    fixed_register_range(
        K1_CCI_BASE
            .checked_add(offset)
            .ok_or(crate::Error::Overflow)?,
        size_of::<u32>(),
    )
}

fn fixed_register_range(start: PhysAddr, len: usize) -> Result<DeviceRegisterRange> {
    PhysAddrRange::from_start_len(start, len).map(DeviceRegisterRange::from_description)
}
