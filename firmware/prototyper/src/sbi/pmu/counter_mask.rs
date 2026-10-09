//! Validated SBI counter selections in the dense hardware/firmware index space.

use runtime::pmu::CounterError;

/// A nonempty set whose selected counter indices all fit the hart's topology.
#[derive(Clone, Copy)]
pub(super) struct CounterMask {
    base: usize,
    bits: usize,
}

impl CounterMask {
    /// Rejects an empty selection, an invalid base, or bits beyond `total_counters`.
    pub(super) fn new(
        base: usize,
        bits: usize,
        total_counters: usize,
    ) -> Result<Self, CounterError> {
        if base >= total_counters || bits == 0 {
            return Err(CounterError::InvalidCounter);
        }
        let available = total_counters - base;
        // If the available range spans XLEN, every representable mask bit fits.
        // Handle that case explicitly to avoid shifting by the integer width.
        if available < usize::BITS as usize && bits >> available != 0 {
            return Err(CounterError::InvalidCounter);
        }
        Ok(Self { base, bits })
    }
}

impl Iterator for CounterMask {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bits == 0 {
            return None;
        }
        let index = self.base + self.bits.trailing_zeros() as usize;
        self.bits &= self.bits - 1;
        Some(index)
    }
}
