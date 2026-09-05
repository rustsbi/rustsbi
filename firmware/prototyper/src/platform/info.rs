//! Platform facts retained after inspecting the Platform Description.

use alloc::string::String;
use alloc::vec::Vec;

use riscv_aia::Iid;
use runtime::SpacemitK1Registers;
use runtime::memory::{DeviceRegisterRange, PhysAddr, PhysAddrRange};

use crate::cfg::NUM_HART_MAX;
use crate::driver;

pub(super) type HartEnableList = [bool; NUM_HART_MAX];

/// Address layout of the machine-level IMSIC interrupt files.
pub(crate) struct ImsicAddressLayout {
    pub(crate) machine_base: PhysAddr,
    pub(crate) hart_index_bits: u32,
    group_index_shift: u32,
    hart_index_shift: u32,
}

impl ImsicAddressLayout {
    pub(super) const fn new(
        machine_base: PhysAddr,
        hart_index_bits: u32,
        group_index_shift: u32,
        hart_index_shift: u32,
    ) -> Self {
        Self {
            machine_base,
            hart_index_bits,
            group_index_shift,
            hart_index_shift,
        }
    }

    pub(super) fn machine_file_address(
        &self,
        hart_index: u32,
        group_index: u32,
    ) -> Option<PhysAddr> {
        let group_offset = if group_index == 0 {
            0
        } else {
            usize::try_from(group_index)
                .ok()?
                .checked_shl(self.group_index_shift)?
        };
        let hart_offset = usize::try_from(hart_index)
            .ok()?
            .checked_shl(self.hart_index_shift)?;
        self.machine_base
            .checked_add(group_offset)?
            .checked_add(hart_offset)
    }
}

/// Machine-level IMSIC resources selected from the Platform Description.
pub(crate) struct ImsicInfo {
    pub(crate) layout: ImsicAddressLayout,
    pub(crate) num_ids: u16,
    pub(crate) ipi_iid: Iid,
    pub(crate) hart_files: [Option<DeviceRegisterRange>; NUM_HART_MAX],
}

/// Console resources selected from the `/chosen/stdout-path` node.
pub(crate) struct ConsoleInfo {
    pub(crate) registers: DeviceRegisterRange,
    pub(crate) kind: driver::ConsoleKind,
    pub(crate) clock_hz: Option<u32>,
}

/// Hardware information used while initializing and serving the platform.
pub(crate) struct BoardInfo {
    pub(crate) ram_ranges: Vec<PhysAddrRange>,
    pub(crate) firmware_ram_range: Option<PhysAddrRange>,
    pub(crate) console: Option<ConsoleInfo>,
    pub(crate) reset: Option<DeviceRegisterRange>,
    pub(crate) clint: Option<(DeviceRegisterRange, driver::ClintKind)>,
    pub(crate) imsic: Option<ImsicInfo>,
    pub(crate) machine_aplic: Option<DeviceRegisterRange>,
    pub(crate) spacemit_k1: Option<SpacemitK1Registers>,
    pub(crate) hart_count: usize,
    pub(crate) timebase_frequency_hz: Option<u32>,
    pub(crate) enabled_harts: HartEnableList,
    pub(crate) model: String,
    pub(crate) pmic_reset: Option<(DeviceRegisterRange, driver::I2cAddress)>,
}

impl BoardInfo {
    pub(super) const fn empty() -> Self {
        Self {
            ram_ranges: Vec::new(),
            firmware_ram_range: None,
            console: None,
            reset: None,
            clint: None,
            imsic: None,
            machine_aplic: None,
            spacemit_k1: None,
            hart_count: 0,
            timebase_frequency_hz: None,
            enabled_harts: [false; NUM_HART_MAX],
            model: String::new(),
            pmic_reset: None,
        }
    }

    pub(crate) fn is_qemu_virt(&self) -> bool {
        self.model == "riscv-virtio,qemu"
    }

    pub(super) fn ram_range_containing(&self, range: PhysAddrRange) -> Option<PhysAddrRange> {
        self.ram_ranges
            .iter()
            .copied()
            .find(|ram| ram.start() <= range.start() && range.end() <= ram.end())
    }
}
