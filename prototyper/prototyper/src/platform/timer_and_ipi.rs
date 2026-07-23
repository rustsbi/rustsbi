//! Coherent selection of the machine timer and notification path.

use super::facts::Platform;

pub(super) type Installed = (
    Option<machine::Timer>,
    Option<machine::Ipi>,
    Option<machine::RemoteFence>,
    Option<machine::HartControl>,
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InstallError {
    IncompleteOrConflictingDescription,
    MachineInstallation,
}

pub(crate) fn install(
    boot: &mut machine::BootInfo,
    facts: &Platform,
) -> Result<Installed, InstallError> {
    if !description_is_coherent(
        facts.imsic.is_some(),
        facts.aplic.is_some(),
        facts.clint.is_some(),
    ) {
        return Err(InstallError::IncompleteOrConflictingDescription);
    }
    match (&facts.imsic, &facts.aplic, &facts.clint) {
        (Some(imsic), Some(aplic), None) => {
            let (timer, ipi, fence, harts) = machine::aia::install(boot, &imsic.path, &aplic.path)
                .map_err(|_| InstallError::MachineInstallation)?;
            Ok((Some(timer), Some(ipi), Some(fence), Some(harts)))
        }
        (None, None, Some(clint)) => {
            let (timer, ipi, fence, harts) = machine::clint::install(boot, &clint.path)
                .map_err(|_| InstallError::MachineInstallation)?;
            Ok((Some(timer), Some(ipi), Some(fence), Some(harts)))
        }
        (None, None, None) => Ok((None, None, None, None)),
        _ => Err(InstallError::IncompleteOrConflictingDescription),
    }
}

fn description_is_coherent(imsic: bool, aplic: bool, clint: bool) -> bool {
    matches!(
        (imsic, aplic, clint),
        (true, true, false) | (false, false, true) | (false, false, false)
    )
}

#[cfg(test)]
#[test]
fn timer_and_ipi_description_accepts_only_complete_paths() {
    assert!(description_is_coherent(true, true, false));
    assert!(description_is_coherent(false, false, true));
    assert!(description_is_coherent(false, false, false));
    assert!(!description_is_coherent(true, false, false));
    assert!(!description_is_coherent(false, true, false));
    assert!(!description_is_coherent(true, true, true));
}
