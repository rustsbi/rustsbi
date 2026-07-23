//! Coherent AIA installation for machine notifications and Sstc deadlines.

use alloc::vec::Vec;

use crate::boot::device_tree::BindingError;
use crate::boot::{BootInfo, MachineRangeError};
use crate::hart::HartAdmission;
use crate::{HartControl, Ipi, RemoteFence, Timer};

mod aplic;
mod imsic;
mod riscv;

/// Failure while installing one selected AIA topology.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallError {
    /// The owned device tree is malformed or an exact node is absent.
    DeviceTree,
    /// The selected nodes do not form a supported AIA path.
    Unsupported,
    /// IMSIC files, APLIC routing, or register ranges are inconsistent.
    InvalidTopology,
    /// The binding is outside the configured trusted platform.
    Unauthorized,
    /// A required sensitive range or singleton runtime was already claimed.
    AlreadyOwned,
    /// A required APLIC register is locked.
    Locked,
    /// A hardware write did not read back as requested.
    Readback,
    /// A required hart-local architectural mechanism is unavailable.
    Hardware,
}

/// Installs one complete IMSIC/APLIC/Sstc path.
///
/// Both exact node identities are reparsed from the owned DTB. No capability
/// is returned until the full topology is validated, all sensitive ranges are
/// claimed, and APLIC routing is configured.
pub fn install(
    boot: &mut BootInfo,
    imsic_path: &str,
    aplic_path: &str,
) -> Result<(Timer, Ipi, RemoteFence, HartControl), InstallError> {
    let imsic = imsic::ImsicTopology::from_dtb(boot, imsic_path).map_err(map_imsic_error)?;
    let aplic = aplic::AplicDescription::from_dtb(boot, aplic_path).map_err(map_aplic_error)?;
    boot.ensure_runtime_unbound()
        .map_err(|_| InstallError::AlreadyOwned)?;

    let mut ranges = Vec::with_capacity(imsic.register_ranges.len() + 1);
    ranges.extend(imsic.register_ranges.iter().cloned());
    ranges.push(aplic.range.clone());
    boot.claim_machine_ranges(&ranges)
        .map_err(|error| match error {
            MachineRangeError::Invalid => InstallError::InvalidTopology,
            MachineRangeError::AlreadyClaimed => InstallError::AlreadyOwned,
        })?;

    let machine_base = imsic
        .register_ranges
        .first()
        .ok_or(InstallError::InvalidTopology)?
        .start;
    aplic
        .configure(machine_base, imsic.hart_index_width)
        .map_err(map_aplic_error)?;
    let harts = imsic.hart_ids();
    let timer = crate::timer::sstc::install(&harts).map_err(|_| InstallError::Hardware)?;
    let device = imsic.into_device();
    let wake_by_ipi = alloc::vec![true; harts.len()];
    let admission = HartAdmission::new(device, &harts, boot.init_hart_id(), &wake_by_ipi)
        .map_err(|_| InstallError::Hardware)?;
    boot.install_runtime(admission.clone(), timer.operations())
        .map_err(|_| InstallError::AlreadyOwned)?;
    Ok((
        timer,
        Ipi::new(admission.clone()),
        RemoteFence::new(admission.clone()),
        HartControl::new(admission),
    ))
}

fn map_imsic_error(error: imsic::ImsicError) -> InstallError {
    match error {
        imsic::ImsicError::Binding(BindingError::DeviceTree) => InstallError::DeviceTree,
        imsic::ImsicError::Binding(BindingError::Unsupported) => InstallError::Unsupported,
        imsic::ImsicError::Binding(BindingError::InvalidRange)
        | imsic::ImsicError::InvalidTopology => InstallError::InvalidTopology,
        imsic::ImsicError::Unauthorized => InstallError::Unauthorized,
        imsic::ImsicError::Hardware => InstallError::Hardware,
    }
}

fn map_aplic_error(error: aplic::AplicError) -> InstallError {
    match error {
        aplic::AplicError::Binding(BindingError::DeviceTree) => InstallError::DeviceTree,
        aplic::AplicError::Binding(BindingError::Unsupported) => InstallError::Unsupported,
        aplic::AplicError::Binding(BindingError::InvalidRange)
        | aplic::AplicError::InvalidConfiguration => InstallError::InvalidTopology,
        aplic::AplicError::Unauthorized => InstallError::Unauthorized,
        aplic::AplicError::Locked => InstallError::Locked,
        aplic::AplicError::Readback => InstallError::Readback,
    }
}
