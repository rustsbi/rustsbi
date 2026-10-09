//! Architectural debug-trigger facts for the current hart.

#![forbid(unsafe_code)]

mod csr;

use alloc::boxed::Box;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Once;

use crate::hart::{HartId, HartIdError};

static COUNTS: Once<Box<[AtomicUsize]>> = Once::new();

/// A debug-trigger discovery operation failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// The operation is not running on the object's published hart.
    InvalidHartId,
    /// A guarded architectural access failed.
    Access(crate::trap::Error),
}

/// Debug-trigger discovery and cached facts for one executing hart.
pub struct DebugTriggers {
    hart: HartId,
    _hart_local: PhantomData<*mut ()>,
}

impl DebugTriggers {
    /// Binds discovery to the current published hart.
    pub fn current() -> Result<Self, HartIdError> {
        Ok(Self {
            hart: HartId::current()?,
            _hart_local: PhantomData,
        })
    }

    /// Returns this hart's discovered debug-trigger count, up to 256.
    ///
    /// An absent trigger block reports zero. Discovery attempts to restore
    /// `tselect` even after a probe error, and returns any restoration error.
    /// Successful counts are cached per hart because trigger blocks can differ.
    /// This operation does not configure any trigger.
    pub fn count(&self) -> Result<usize, Error> {
        let hart = HartId::current().map_err(|_| Error::InvalidHartId)?;
        if hart != self.hart {
            return Err(Error::InvalidHartId);
        }
        let counts = COUNTS.call_once(|| {
            HartId::all()
                .map(|_| AtomicUsize::new(usize::MAX))
                .collect()
        });
        let cache = &counts[hart.index()];
        let cached = cache.load(Ordering::Relaxed);
        if cached != usize::MAX {
            return Ok(cached);
        }
        let count = csr::probe_count().map_err(Error::Access)?;
        cache.store(count, Ordering::Relaxed);
        Ok(count)
    }
}
