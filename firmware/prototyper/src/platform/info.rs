//! Platform facts retained after inspecting the Platform Description.
//!
//! [`BoardInfo`] groups facts by their policy consumer so generic boot code
//! does not depend on individual reset or interrupt-controller models.

use alloc::string::String;
use alloc::vec::Vec;

use riscv_aia::Iid;
use runtime::SpacemitK1Registers;
use runtime::memory::{DeviceRegisterRange, PhysAddr, PhysAddrRange};

use crate::cfg::NUM_HART_MAX;
use crate::devicetree::{NodeSelection, select_from_node_once};
use crate::driver;

pub(super) type HartEnableList = [bool; NUM_HART_MAX];

/// Per-hart register index assigned by one ACLINT device, keyed by hart ID.
///
/// The ACLINT specification says a device assigns a HART index starting from
/// zero to each HART connected to it, and that this index "may or may not have
/// any relationship" with the hart ID. The index of a hart is therefore taken
/// from the device's interrupt list instead of being assumed equal to its ID.
#[derive(Clone, Copy)]
pub(crate) struct HartIndexMap {
    indices: [Option<u32>; NUM_HART_MAX],
    count: u32,
}

impl HartIndexMap {
    pub(crate) const fn new() -> Self {
        Self {
            indices: [None; NUM_HART_MAX],
            count: 0,
        }
    }

    /// Records the device register index `index` for `hart_id`.
    pub(crate) fn insert(&mut self, hart_id: usize, index: u32) -> bool {
        let Some(slot) = self.indices.get_mut(hart_id) else {
            return false;
        };
        if slot.is_some() {
            return false;
        }
        *slot = Some(index);
        self.count += 1;
        true
    }

    /// Returns the device register index of `hart_id`.
    pub(crate) fn get(&self, hart_id: usize) -> Option<u32> {
        self.indices.get(hart_id).copied().flatten()
    }

    /// Returns how many harts this device serves.
    pub(crate) const fn count(self) -> u32 {
        self.count
    }
}

/// One ACLINT MSWI device: its IPI register window plus per-hart indexing.
#[derive(Clone, Copy)]
pub(crate) struct AclintMswi {
    pub(crate) registers: DeviceRegisterRange,
    pub(crate) hart_indices: HartIndexMap,
}

/// One ACLINT MTIMER device: its compare window, its optional time window, and
/// per-hart indexing.
#[derive(Clone, Copy)]
pub(crate) struct AclintMtimer {
    pub(crate) compare: DeviceRegisterRange,
    pub(crate) time: Option<DeviceRegisterRange>,
    pub(crate) hart_indices: HartIndexMap,
}

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

/// Console resources selected from the device tree.
pub(crate) struct ConsoleInfo {
    pub(crate) registers: DeviceRegisterRange,
    pub(crate) kind: driver::ConsoleKind,
    pub(crate) clock_hz: Option<u32>,
}

/// Memory layout retained for firmware policy and platform reporting.
pub(crate) struct MemoryInfo {
    pub(crate) ram_ranges: Vec<PhysAddrRange>,
    pub(crate) firmware_ram_range: Option<PhysAddrRange>,
    pub(crate) noncacheable_alias_offset: Option<u64>,
}

impl MemoryInfo {
    fn empty() -> Self {
        Self {
            ram_ranges: Vec::new(),
            firmware_ram_range: None,
            noncacheable_alias_offset: None,
        }
    }

    pub(super) fn ram_range_containing(&self, range: PhysAddrRange) -> Option<PhysAddrRange> {
        self.ram_ranges
            .iter()
            .copied()
            .find(|ram| ram.start() <= range.start() && range.end() <= ram.end())
    }
}

/// Hart topology and architectural timer frequency.
pub(crate) struct HartInfo {
    pub(crate) count: usize,
    pub(crate) timebase_frequency_hz: Option<u32>,
    pub(crate) enabled: HartEnableList,
}

impl HartInfo {
    const fn empty() -> Self {
        Self {
            count: 0,
            timebase_frequency_hz: None,
            enabled: [false; NUM_HART_MAX],
        }
    }
}

/// The CLINT register range and driver-specific interpretation.
pub(crate) struct ClintResource {
    pub(crate) registers: DeviceRegisterRange,
    pub(crate) kind: driver::ClintKind,
}

/// Whether the machine APLIC node is hidden from the next-stage FDT.
#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub(crate) enum MachineAplicHandoff {
    #[default]
    KeepVisible,
    Hide,
}

/// Interrupt-controller descriptions collected before device binding.
///
/// CLINT, IMSIC, and machine APLIC retain their source paths because the
/// selected IMSIC policy may hide those nodes from the next-stage FDT. The
/// legacy controller slots below only need their register ranges for binding.
pub(crate) struct InterruptDescriptions {
    clint: Option<NodeSelection<ClintResource>>,
    imsic: Option<NodeSelection<ImsicInfo>>,
    machine_aplic: Option<NodeSelection<DeviceRegisterRange>>,
    machine_aplic_handoff: MachineAplicHandoff,
    pub(crate) thead_plic: Option<DeviceRegisterRange>,
    pub(crate) plmt: Option<DeviceRegisterRange>,
    pub(crate) plicsw: Option<DeviceRegisterRange>,
    pub(crate) aclint_mswi: Option<AclintMswi>,
    pub(crate) aclint_mtimer: Option<AclintMtimer>,
}

impl InterruptDescriptions {
    pub(crate) fn clint(&self) -> Option<&NodeSelection<ClintResource>> {
        self.clint.as_ref()
    }

    pub(crate) fn imsic(&self) -> Option<&NodeSelection<ImsicInfo>> {
        self.imsic.as_ref()
    }

    pub(crate) fn machine_aplic(&self) -> Option<&NodeSelection<DeviceRegisterRange>> {
        self.machine_aplic.as_ref()
    }

    pub(crate) fn set_clint(
        &mut self,
        value: ClintResource,
        source_path: &[&str],
    ) -> runtime::Result<()> {
        select_from_node_once(&mut self.clint, value, source_path)
    }

    pub(crate) fn set_imsic(
        &mut self,
        value: ImsicInfo,
        source_path: &[&str],
    ) -> runtime::Result<()> {
        select_from_node_once(&mut self.imsic, value, source_path)
    }

    pub(crate) fn set_machine_aplic(
        &mut self,
        value: DeviceRegisterRange,
        source_path: &[&str],
        handoff: MachineAplicHandoff,
    ) -> runtime::Result<()> {
        select_from_node_once(&mut self.machine_aplic, value, source_path)?;
        self.machine_aplic_handoff = handoff;
        Ok(())
    }

    /// Returns source paths hidden when firmware selects IMSIC for IPIs.
    pub(crate) fn aia_handoff_paths(&self) -> impl Iterator<Item = &str> + '_ {
        let machine_aplic = match self.machine_aplic_handoff {
            MachineAplicHandoff::KeepVisible => None,
            MachineAplicHandoff::Hide => self
                .machine_aplic
                .as_ref()
                .map(|selection| selection.source_path()),
        };
        [
            self.clint.as_ref().map(|selection| selection.source_path()),
            self.imsic.as_ref().map(|selection| selection.source_path()),
            machine_aplic,
        ]
        .into_iter()
        .flatten()
    }

    const fn empty() -> Self {
        Self {
            clint: None,
            imsic: None,
            machine_aplic: None,
            machine_aplic_handoff: MachineAplicHandoff::KeepVisible,
            thead_plic: None,
            plmt: None,
            plicsw: None,
            aclint_mswi: None,
            aclint_mtimer: None,
        }
    }
}

/// Unbound descriptions for the platform-wide device classes.
pub(crate) struct DeviceDescriptions {
    pub(crate) console: Option<ConsoleInfo>,
    pub(crate) reset: driver::ResetDescription,
    pub(crate) interrupts: InterruptDescriptions,
}

impl DeviceDescriptions {
    fn empty() -> Self {
        Self {
            console: None,
            reset: driver::ResetDescription::empty(),
            interrupts: InterruptDescriptions::empty(),
        }
    }
}

/// A supported root-level SoC description selected during discovery.
pub(crate) enum SocDescription {
    SpacemitK1(SpacemitK1Registers),
    V821(crate::platform::allwinner::v821::Description),
    V861(runtime::soc::allwinner::v861::AllwinnerV861Soc),
}

/// Platform facts grouped by the policy that consumes them.
pub(crate) struct BoardInfo {
    pub(crate) model: String,
    pub(crate) memory: MemoryInfo,
    pub(crate) harts: HartInfo,
    pub(crate) devices: DeviceDescriptions,
    pub(crate) soc: Option<SocDescription>,
}

impl BoardInfo {
    pub(super) fn empty() -> Self {
        Self {
            model: String::new(),
            memory: MemoryInfo::empty(),
            harts: HartInfo::empty(),
            devices: DeviceDescriptions::empty(),
            soc: None,
        }
    }

    pub(crate) fn is_qemu_virt(&self) -> bool {
        self.model == "riscv-virtio,qemu"
    }
}
