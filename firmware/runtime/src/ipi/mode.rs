//! Runtime interrupt operations selected by an IPI's mode.
//!
//! Context lookup stays private so platform callbacks cannot obtain an IMSIC.

use super::{External, Imsic, Software, imsic};
use crate::{Error, Result, irq::LocalIrqDisabled};
use csr::machine;

/// Per-mode contexts, initialization, and interrupt enables.
pub trait InterruptMode: Sized + 'static {
    type Context;

    const SOFTWARE_ENABLE: machine::Interrupt;
    const EXTERNAL_ENABLE: machine::Interrupt;

    fn context(irq: &LocalIrqDisabled) -> Result<&Self::Context>;

    fn init_hart(_irq: &LocalIrqDisabled) -> Result<()> {
        Ok(())
    }

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

    fn context(_irq: &LocalIrqDisabled) -> Result<&Self::Context> {
        Ok(&())
    }
}

impl InterruptMode for External {
    type Context = Imsic;

    const SOFTWARE_ENABLE: machine::Interrupt = machine::Interrupt::Disabled;
    const EXTERNAL_ENABLE: machine::Interrupt = machine::Interrupt::Enabled;

    fn init_hart(irq: &LocalIrqDisabled) -> Result<()> {
        imsic::get().ok_or(Error::NotReady)?.init_hart(irq)
    }

    fn context(_irq: &LocalIrqDisabled) -> Result<&Self::Context> {
        imsic::get().ok_or(Error::NotReady)
    }
}
