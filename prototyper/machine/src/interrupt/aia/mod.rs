//! Machine ownership of one firmware-selected AIA interrupt path.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::Range;

use crate::boot::BootInfo;
use crate::hart::HartAdmission;
use crate::{HartControl, Interrupts, IoMem, Ipi, RemoteFence};

mod aplic;
mod imsic;

/// One machine interrupt file selected from the firmware-owned device tree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImsicFile {
    /// Physical hart served by this file.
    pub hart_id: usize,
    /// Start address of that hart's 4 KiB machine interrupt file.
    pub address: usize,
}

/// The APLIC register block selected for one AIA path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AplicLayout {
    range: Range<usize>,
    source_count: u32,
    supervisor_imsic_base: u64,
}

impl AplicLayout {
    /// Records the concrete routing block that firmware selected.
    pub fn new(range: Range<usize>, source_count: u32, supervisor_imsic_base: u64) -> Option<Self> {
        (range.start < range.end
            && range.start.is_multiple_of(4)
            && range.end.is_multiple_of(4)
            && source_count != 0
            && supervisor_imsic_base.is_multiple_of(0x1000))
        .then_some(Self {
            range,
            source_count,
            supervisor_imsic_base,
        })
    }
}

/// Complete AIA facts selected and decoded by firmware.
///
/// This is deliberately a layout, not a device-tree adapter: the machine
/// crate owns only the device windows and architectural programming sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiaLayout {
    imsic_ranges: Vec<Range<usize>>,
    files: Vec<ImsicFile>,
    interrupt_identity_count: u16,
    notification_identity: u16,
    hart_index_width: u32,
    aplic: AplicLayout,
}

impl AiaLayout {
    /// Creates one fully specified machine AIA installation request.
    pub fn new(
        imsic_ranges: Vec<Range<usize>>,
        files: Vec<ImsicFile>,
        interrupt_identity_count: u16,
        notification_identity: u16,
        hart_index_width: u32,
        aplic: AplicLayout,
    ) -> Option<Self> {
        if imsic_ranges.is_empty()
            || files.is_empty()
            || interrupt_identity_count <= notification_identity
            || interrupt_identity_count > 2048
            || hart_index_width > 7
            || imsic_ranges.iter().any(|range| {
                range.start >= range.end
                    || !range.start.is_multiple_of(0x1000)
                    || !(range.end - range.start).is_multiple_of(0x1000)
            })
        {
            return None;
        }
        for (index, file) in files.iter().enumerate() {
            let end = file.address.checked_add(0x1000)?;
            if !file.address.is_multiple_of(0x1000)
                || !imsic_ranges
                    .iter()
                    .any(|range| file.address >= range.start && end <= range.end)
                || files[..index]
                    .iter()
                    .any(|other| other.hart_id == file.hart_id || other.address == file.address)
            {
                return None;
            }
        }
        Some(Self {
            imsic_ranges,
            files,
            interrupt_identity_count,
            notification_identity,
            hart_index_width,
            aplic,
        })
    }
}

/// Claims and initializes the one AIA path selected by firmware.
///
/// A failed setup leaves no machine capability available to upper firmware;
/// the caller owns the terminal boot-policy decision.
pub fn install(boot: &mut BootInfo, layout: AiaLayout) -> Option<Interrupts> {
    let mut imsic_windows = Vec::with_capacity(layout.imsic_ranges.len());
    for range in layout.imsic_ranges {
        imsic_windows.push(IoMem::acquire(boot, range)?);
    }
    let aplic = IoMem::acquire(boot, layout.aplic.range.clone())?;
    let machine_imsic_base = imsic_windows.first()?.range().start;
    aplic::configure(
        &aplic,
        layout.aplic.source_count,
        u64::try_from(machine_imsic_base).ok()?,
        layout.aplic.supervisor_imsic_base,
        layout.hart_index_width,
    )?;

    let harts: Vec<_> = layout.files.iter().map(|file| file.hart_id).collect();
    let timer = crate::timer::sstc::install(&harts).ok()?;
    let device = Arc::new(imsic::Imsic::new(
        imsic_windows,
        layout.files,
        layout.interrupt_identity_count,
        layout.notification_identity,
    ));
    let wake_by_ipi = alloc::vec![true; harts.len()];
    let runtime = HartAdmission::new(device, &harts, boot.init_hart_id(), &wake_by_ipi).ok()?;
    boot.install_runtime(runtime.clone(), timer.operations())
        .then_some(())?;

    Some(Interrupts {
        timer,
        ipi: Ipi::new(runtime.clone()),
        remote_fence: RemoteFence::new(runtime.clone()),
        harts: HartControl::new(runtime),
    })
}
