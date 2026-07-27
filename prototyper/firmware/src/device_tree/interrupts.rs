//! Coherent discovery and installation of timer and hart-notification services.

use super::Error;
use super::aia;
use super::clint;
use super::dt::BootTree;

pub(crate) fn install(
    boot: &mut machine::BootInfo,
    tree: &mut BootTree,
    harts: &[usize],
) -> Result<Option<machine::Interrupts>, Error> {
    let (imsic, aplic) = aia::discover(tree, harts)?;
    let clint = clint::discover(&tree.tree().root, harts)?;
    if !description_is_coherent(imsic.is_some(), aplic.is_some(), clint.is_some()) {
        return Err(Error::UnsupportedDevice);
    }

    match (imsic, aplic, clint) {
        (Some(imsic), Some(aplic), None) => {
            let layout = machine::interrupt::aia::AiaLayout::new(
                imsic.ranges,
                imsic.files,
                imsic.interrupt_identity_count,
                1,
                imsic.hart_index_width,
                machine::interrupt::aia::AplicLayout::new(
                    aplic.range,
                    aplic.source_count,
                    imsic.supervisor_imsic_base,
                )
                .ok_or(Error::Installation)?,
            )
            .ok_or(Error::Installation)?;
            let installed =
                machine::interrupt::aia::install(boot, layout).ok_or(Error::Installation)?;
            tree.remove_node(&aplic.path)?;
            tree.remove_node(&imsic.path)?;
            Ok(Some(installed))
        }
        (None, None, Some(clint)) => {
            let installed = machine::interrupt::clint::install(boot, clint.layout)
                .ok_or(Error::Installation)?;
            tree.remove_node(&clint.path)?;
            Ok(Some(installed))
        }
        (None, None, None) => Ok(None),
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
fn interrupt_description_accepts_only_complete_paths() {
    assert!(description_is_coherent(true, true, false));
    assert!(description_is_coherent(false, false, true));
    assert!(description_is_coherent(false, false, false));
    assert!(!description_is_coherent(true, false, false));
    assert!(!description_is_coherent(false, true, false));
    assert!(!description_is_coherent(true, true, true));
}
