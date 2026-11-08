//! SpacemiT K1 system-register descriptions.
//!
//! Compatibility reference: the pinned [OpenSBI K1 platform header] defines
//! the SpacemiT K1 addresses used here.
//!
//! [OpenSBI K1 platform header]: https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/platform/generic/include/spacemit/k1.h

use core::mem::size_of;

use fdt::node::FdtNode;

use crate::memory::{PhysAddr, RegRange};
use crate::{Error, Result};

const SPACEMIT_K1_COMPATIBLE: &str = "spacemit,k1x";
const K1_RESET_VECTOR_BASES: [PhysAddr; 2] =
    [PhysAddr::new(0xd428_2db0), PhysAddr::new(0xd428_2eb0)];
const K1_CCI_BASE: PhysAddr = PhysAddr::new(0xd850_0000);
const K1_CCI_FIRST_INTERFACE_OFFSET: usize = 0x1000;
const K1_CCI_INTERFACE_STRIDE: usize = 0x1000;
const K1_HARTS_PER_CLUSTER: usize = 4;
const K1_CORE_STATUS: PhysAddr = PhysAddr::new(0xd428_2890);
const K1_WAKEUP_BASES: [PhysAddr; 2] = [PhysAddr::new(0xd428_292c), PhysAddr::new(0xd428_2b24)];

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
    reset_vectors: [RegRange; 2],
    cci_status: RegRange,
    cci_snoop_controls: [RegRange; 2],
    core_status: RegRange,
    wakeup_controls: [RegRange; 2],
}

impl SpacemitK1Registers {
    /// Enables K1 hart-local cache access before a warm entry touches memory.
    ///
    /// # Safety
    /// Called only on K1 in M-mode, with cluster coherency already enabled.
    /// This stackless entry clobbers t0/t1 and must be called from assembly.
    #[cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), unsafe(naked))]
    pub unsafe extern "C" fn prepare_warm_hart() {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        core::arch::naked_asm!(
            "csrr t0, mhartid",
            "andi t0, t0, 3",
            "li t1, 1",
            "sll t1, t1, t0",
            "csrs {l2}, t1",
            "li t0, {features}",
            "csrs {setup}, t0",
            "ret",
            l2 = const K1Csr::MachineL2Setup as u16,
            setup = const K1Csr::MachineSetup as u16,
            features = const MachineSetup::enabled().0,
        );
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        unimplemented!("SpacemiT K1 warm-hart preparation requires a RISC-V target");
    }

    /// Returns K1 system registers when the root compatible list identifies K1.
    pub(crate) fn from_root(root: FdtNode<'_, '_>) -> Result<Option<Self>> {
        let Some(compatible) = root.compatible() else {
            return Ok(None);
        };
        if !compatible
            .all()
            .any(|value| value == SPACEMIT_K1_COMPATIBLE)
        {
            return Ok(None);
        }

        let reset_vector_span = ResetVectorRegister::AddressHigh as usize + size_of::<u32>();
        let reset_vectors = [
            RegRange::from_start_len(K1_RESET_VECTOR_BASES[0], reset_vector_span)?,
            RegRange::from_start_len(K1_RESET_VECTOR_BASES[1], reset_vector_span)?,
        ];
        let cci_status = RegRange::from_start_len(
            K1_CCI_BASE
                .checked_add(CciRegister::Status as usize)
                .ok_or(Error::Overflow)?,
            size_of::<u32>(),
        )?;
        let cci_snoop_controls = [cci_snoop_control_range(0)?, cci_snoop_control_range(1)?];

        Ok(Some(Self {
            reset_vectors,
            cci_status,
            cci_snoop_controls,
            core_status: RegRange::from_start_len(K1_CORE_STATUS, size_of::<u32>())?,
            wakeup_controls: [
                RegRange::from_start_len(K1_WAKEUP_BASES[0], 4 * size_of::<u32>())?,
                RegRange::from_start_len(K1_WAKEUP_BASES[1], 4 * size_of::<u32>())?,
            ],
        }))
    }

    /// Returns the two cluster reset-vector register ranges.
    pub const fn reset_vectors(self) -> [RegRange; 2] {
        self.reset_vectors
    }

    /// Returns the CCI status-register range.
    pub const fn cci_status(self) -> RegRange {
        self.cci_status
    }

    /// Returns the two cluster CCI snoop-control ranges.
    pub const fn cci_snoop_controls(self) -> [RegRange; 2] {
        self.cci_snoop_controls
    }

    /// Returns the `PMU_CORE_STATUS` register range (K1 User Manual, section 9.9.4.9.3).
    pub const fn core_status(self) -> RegRange {
        self.core_status
    }

    /// Returns the per-caller wakeup register ranges (K1 User Manual, section 9.9.4.9.12).
    pub const fn wakeup_controls(self) -> [RegRange; 2] {
        self.wakeup_controls
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

#[inline]
fn set_csr<const CSR: u16>(bits: usize) {
    match () {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        // SAFETY: The K1 capability identifies these implementation-defined
        // CSRs. Firmware invokes these methods in M-mode on the current hart.
        () => unsafe {
            core::arch::asm!(
                "csrs {csr}, {bits}",
                csr = const CSR,
                bits = in(reg) bits,
                options(nomem)
            );
        },
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        () => {
            let _ = (CSR, bits);
            unimplemented!("SpacemiT K1 CSR access requires a RISC-V target");
        }
    }
}

fn cci_snoop_control_range(interface_index: usize) -> Result<RegRange> {
    let offset = K1_CCI_INTERFACE_STRIDE
        .checked_mul(interface_index)
        .and_then(|offset| K1_CCI_FIRST_INTERFACE_OFFSET.checked_add(offset))
        .and_then(|offset| offset.checked_add(CciRegister::SnoopControl as usize))
        .ok_or(Error::Overflow)?;
    RegRange::from_start_len(
        K1_CCI_BASE.checked_add(offset).ok_or(Error::Overflow)?,
        size_of::<u32>(),
    )
}
