//! Runtime interrupt operations selected by an IPI's mode.

use super::{External, Imsic, Software};
use crate::irq::LocalIrqDisabled;
use csr::machine;

/// Per-mode contexts and interrupt enables.
pub trait InterruptMode: Sized + 'static {
    type Context;

    const SOFTWARE_ENABLE: machine::Interrupt;
    const EXTERNAL_ENABLE: machine::Interrupt;

    fn enable_interrupt(_irq: &LocalIrqDisabled) {
        // SAFETY:
        // 1. Boot installed the Runtime trap entry before enabling wake sources.
        // 2. The IRQ context excludes interrupt reentry during this update.
        unsafe {
            machine::InterruptEnable::modify(|_, value| {
                value.set::<machine::MSIE>(Self::SOFTWARE_ENABLE);
                value.set::<machine::MEIE>(Self::EXTERNAL_ENABLE);
            });
        }
    }
}

impl InterruptMode for Software {
    type Context = ();

    const SOFTWARE_ENABLE: machine::Interrupt = machine::Interrupt::Enabled;
    const EXTERNAL_ENABLE: machine::Interrupt = machine::Interrupt::Disabled;
}

impl InterruptMode for External {
    type Context = Imsic;

    const SOFTWARE_ENABLE: machine::Interrupt = machine::Interrupt::Disabled;
    const EXTERNAL_ENABLE: machine::Interrupt = machine::Interrupt::Enabled;
}
