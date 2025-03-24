use super::sbi_ret::SbiRegister;

/// Debug trigger mask structure for the `DBTR` extension §19.
#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct TriggerMask<T = usize> {
    trig_idx_base: T,
    trig_idx_mask: T,
}

impl<T: SbiRegister> TriggerMask<T> {
    /// Construct a [TriggerMask] from mask value and base counter index.
    ///
    /// The `trig_idx_base` specifies the starting trigger index, while the `trig_idx_mask` is a
    /// bitmask indicating which triggers, relative to the base, are to be operated.
    #[inline]
    pub const fn from_mask_base(trig_idx_mask: T, trig_idx_base: T) -> Self {
        Self {
            trig_idx_mask,
            trig_idx_base,
        }
    }

    /// Returns `mask` and `base` parameters from the [TriggerMask].
    #[inline]
    pub const fn into_inner(self) -> (T, T) {
        (self.trig_idx_mask, self.trig_idx_base)
    }
}
