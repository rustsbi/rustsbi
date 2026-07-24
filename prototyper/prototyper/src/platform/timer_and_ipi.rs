//! Coherent discovery and installation of timer and hart-notification services.

use super::aia;
use super::clint;
use super::discovery::Error;
use super::dt::BootTree;
use super::hart::HartInfo;

pub(super) type Installed = (
    Option<machine::Timer>,
    Option<machine::Ipi>,
    Option<machine::RemoteFence>,
    Option<machine::HartControl>,
);

pub(crate) fn install(
    boot: &mut machine::BootInfo,
    tree: &mut BootTree,
    harts: &[HartInfo],
) -> Result<Installed, Error> {
    let (imsic, aplic) = aia::discover(tree, harts)?;
    let clint = clint::discover(&tree.tree().root)?;
    if !description_is_coherent(imsic.is_some(), aplic.is_some(), clint.is_some()) {
        return Err(Error::UnsupportedDevice);
    }

    match (imsic, aplic, clint) {
        (Some(imsic), Some(aplic), None) => {
            let installed = machine::aia::install(boot, &imsic.path, &aplic.path)
                .map_err(|_| Error::Installation)?;
            tree.remove_node(&aplic.path)?;
            tree.remove_node(&imsic.path)?;
            let (timer, ipi, fence, harts) = installed;
            Ok((Some(timer), Some(ipi), Some(fence), Some(harts)))
        }
        (None, None, Some(clint)) => {
            let installed =
                machine::clint::install(boot, &clint.path).map_err(|_| Error::Installation)?;
            tree.remove_node(&clint.path)?;
            let (timer, ipi, fence, harts) = installed;
            Ok((Some(timer), Some(ipi), Some(fence), Some(harts)))
        }
        (None, None, None) => Ok((None, None, None, None)),
        _ => Err(Error::UnsupportedDevice),
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
