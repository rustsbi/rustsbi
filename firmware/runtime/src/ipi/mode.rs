//! Acknowledgement context for each IPI delivery mode.

use super::{External, Imsic, Software};

/// Context required to acknowledge the interrupt.
pub trait InterruptMode: Sized + 'static {
    type Context;
}

impl InterruptMode for Software {
    type Context = ();
}

impl InterruptMode for External {
    type Context = Imsic;
}
