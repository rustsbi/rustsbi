//! Platform facts retained after inspecting the Platform Description.
//!
//! [`PlatformInfo`] groups facts by their policy consumer so generic boot code
//! does not depend on individual reset or interrupt-controller models.

use alloc::string::String;
use alloc::vec::Vec;
use runtime::{
    Result,
    memory::{DeviceRegisterRange, PhysAddrRange},
};

use crate::devicetree::{NodeSelection, select_from_node_once};
use crate::driver::DeviceSummary;
use crate::driver::irqchip::aia::ImsicInfo;

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

    pub(crate) fn ram_range_containing(&self, range: PhysAddrRange) -> Option<PhysAddrRange> {
        self.ram_ranges
            .iter()
            .copied()
            .find(|ram| ram.start() <= range.start() && range.end() <= ram.end())
    }
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
    clint: Option<NodeSelection<DeviceRegisterRange>>,
    imsic: Option<NodeSelection<ImsicInfo>>,
    machine_aplic: Option<NodeSelection<DeviceRegisterRange>>,
    machine_aplic_handoff: MachineAplicHandoff,
    pub(crate) thead_plic: Option<DeviceRegisterRange>,
    pub(crate) plmt: Option<DeviceRegisterRange>,
    pub(crate) plicsw: Option<DeviceRegisterRange>,
}

impl InterruptDescriptions {
    pub(crate) fn clint(&self) -> Option<&NodeSelection<DeviceRegisterRange>> {
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
        value: DeviceRegisterRange,
        source_path: &[&str],
    ) -> Result<()> {
        select_from_node_once(&mut self.clint, value, source_path)
    }

    pub(crate) fn set_imsic(&mut self, value: ImsicInfo, source_path: &[&str]) -> Result<()> {
        select_from_node_once(&mut self.imsic, value, source_path)
    }

    pub(crate) fn set_machine_aplic(
        &mut self,
        value: DeviceRegisterRange,
        source_path: &[&str],
        handoff: MachineAplicHandoff,
    ) -> Result<()> {
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
        }
    }
}

/// Unbound descriptions for the platform-wide device classes.
pub(crate) struct DeviceDescriptions {
    pub(crate) interrupts: InterruptDescriptions,
}

impl DeviceDescriptions {
    fn empty() -> Self {
        Self {
            interrupts: InterruptDescriptions::empty(),
        }
    }
}

/// Platform facts grouped by the policy that consumes them.
pub(crate) struct PlatformInfo {
    pub(crate) model: String,
    pub(crate) memory: MemoryInfo,
    pub(crate) timebase_frequency_hz: Option<u32>,
    pub(crate) devices: DeviceDescriptions,
    pub(crate) console_summary: Option<crate::driver::ConsoleSummary>,
    pub(crate) clint_summary: Option<DeviceSummary>,
}

impl PlatformInfo {
    pub(crate) fn empty() -> Self {
        Self {
            model: String::new(),
            memory: MemoryInfo::empty(),
            timebase_frequency_hz: None,
            devices: DeviceDescriptions::empty(),
            console_summary: None,
            clint_summary: None,
        }
    }
}
