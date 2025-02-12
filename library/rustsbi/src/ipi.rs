use sbi_spec::binary::{HartMask, SbiRet};

/// Inter-processor interrupt support.
pub trait Ipi {
    /// Send an inter-processor interrupt to all the harts defined in `hart_mask`.
    ///
    /// Inter-processor interrupts manifest at the receiving harts as the supervisor software interrupts.
    ///
    /// # Return value
    ///
    /// Should return `SbiRet::success()` if IPI was sent to all the targeted harts successfully.
    fn send_ipi(&self, hart_mask: HartMask) -> SbiRet;
    /// Function internal to macros. Do not use.
    #[doc(hidden)]
    #[inline]
    fn _rustsbi_probe(&self) -> usize {
        sbi_spec::base::UNAVAILABLE_EXTENSION.wrapping_add(1)
    }
}

impl<T: Ipi> Ipi for &T {
    #[inline]
    fn send_ipi(&self, hart_mask: HartMask) -> SbiRet {
        T::send_ipi(self, hart_mask)
    }
}

impl<T: Ipi> Ipi for Option<T> {
    #[inline]
    fn send_ipi(&self, hart_mask: HartMask) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::send_ipi(inner, hart_mask)
        })
    }
    #[inline]
    fn _rustsbi_probe(&self) -> usize {
        match self {
            Some(_) => sbi_spec::base::UNAVAILABLE_EXTENSION.wrapping_add(1),
            None => sbi_spec::base::UNAVAILABLE_EXTENSION,
        }
    }
}
