//! Borrowed interrupt services owned by the published firmware policy.

use crate::{Error, Result, ipi::Ipi, timer::Timer};
use spin::Once;

struct Services {
    timer: &'static Timer,
    ipi: &'static Ipi,
}

static SERVICES: Once<Services> = Once::new();

/// Registers services borrowed from their permanently stored owner.
/// Call once before configuring harts or enabling machine interrupt sources.
/// Duplicate registration preserves the original services.
pub fn install_services(timer: &'static Timer, ipi: &'static Ipi) -> Result<()> {
    publish(&SERVICES, Services { timer, ipi })
}

fn publish(slot: &Once<Services>, services: Services) -> Result<()> {
    let mut installed = false;
    slot.call_once(|| {
        installed = true;
        services
    });
    if installed {
        Ok(())
    } else {
        Err(Error::AlreadyInitialized)
    }
}

pub(crate) fn timer() -> Option<&'static Timer> {
    SERVICES.get().map(|services| services.timer)
}

pub(crate) fn ipi() -> Option<&'static Ipi> {
    SERVICES.get().map(|services| services.ipi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;

    #[test]
    fn duplicate_registration_preserves_both_original_borrows() {
        let slot = Once::new();
        let timer = Box::leak(Box::new(Timer::new(None)));
        let ipi = Box::leak(Box::new(Ipi::new(None, || false)));
        assert_eq!(publish(&slot, Services { timer, ipi }), Ok(()));
        let replacement_timer = Box::leak(Box::new(Timer::new(None)));
        let replacement_ipi = Box::leak(Box::new(Ipi::new(None, || false)));
        assert_eq!(
            publish(
                &slot,
                Services {
                    timer: replacement_timer,
                    ipi: replacement_ipi
                }
            ),
            Err(Error::AlreadyInitialized)
        );
        let registered = slot.get().unwrap();
        assert!(core::ptr::eq(registered.timer, timer));
        assert!(core::ptr::eq(registered.ipi, ipi));
    }
}
