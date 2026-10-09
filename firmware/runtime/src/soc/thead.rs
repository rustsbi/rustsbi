//! T-Head machine-state mechanisms shared by its implementation-defined cores.

use core::marker::PhantomData;

use crate::csr::{Readable, Writable, registers};
use crate::hart::{HartId, HartIdError};

const THEAD_VENDOR_ID: usize = 0x5b7;
const MEMORY_ATTRIBUTE_EXTENSION: usize = 1 << 21;

registers! {
    read {}
    write {
        Mxstatus: usize = 0x7c0;
    }
}

/// Access to the current T-Head hart's implementation-defined machine state.
///
/// The vendor ID authorizes this capability. It cannot move or be shared
/// between harts; obtaining another handle on the same hart is permitted.
pub struct THead {
    hart: HartId,
    _hart_local: PhantomData<*mut ()>,
}

impl THead {
    /// Binds the current hart when its architectural vendor ID identifies T-Head.
    pub fn current() -> Result<Option<Self>, HartIdError> {
        let hart = HartId::current()?;
        Ok(
            (crate::hart::vendor_id() == THEAD_VENDOR_ID).then_some(Self {
                hart,
                _hart_local: PhantomData,
            }),
        )
    }

    /// Disables MAEE so supervisor page tables use standard memory attributes.
    ///
    /// An absent optional `mxstatus` needs no update. Other guarded-access
    /// failures are returned, and all unrelated register bits are retained.
    ///
    /// # Panics
    ///
    /// Panics if this capability is used on a different hart.
    pub fn use_standard_page_memory_types(&self) -> Result<(), crate::trap::Error> {
        assert_eq!(
            HartId::current(),
            Ok(self.hart),
            "BUG: T-Head capability used on another hart"
        );
        riscv::interrupt::machine::free(|| {
            if let Some(current) = Mxstatus::read_optional()? {
                Mxstatus::write(current & !MEMORY_ATTRIBUTE_EXTENSION)?;
            }
            Ok(())
        })
    }
}
