use crate::SbiRet;

/// One vendor-specific SBI extension handler.
///
/// Fields implementing this trait are bound to EIDs by
/// [`derive(VendorSBI)`](crate::VendorSBI). `Option<T>` makes a handler optional,
/// and references forward to their underlying implementation.
pub trait Extension {
    /// Returns the BASE probe value; zero means this extension is unavailable.
    ///
    /// Keep this query free of side effects: dispatch also uses it to check availability.
    fn probe(&self) -> usize;

    /// Handles a function after this extension has been selected by its EID.
    ///
    /// Return [`SbiRet::not_supported`] for unknown functions. The dispatcher returns
    /// this result unchanged and does not try another extension on failure.
    fn handle(&self, fid: usize, args: [usize; 6]) -> SbiRet;
}

/// Routes vendor-specific SBI extension calls by EID.
///
/// Derive this trait on a struct containing `#[rustsbi(extension(eid = EID))]`
/// fields. A [`RustSBI`](crate::RustSBI) dispatcher can then contain exactly one
/// `#[rustsbi(vendor)]` field implementing this trait. Deriving it on an enum whose
/// variants each contain one unnamed field creates a vendor selector.
///
/// Duplicate EIDs, standard EIDs, legacy EIDs, and values wider than 32 bits are
/// rejected at compile time:
///
/// ```compile_fail,E0080
/// # use rustsbi::{Extension, SbiRet, VendorSBI};
/// # struct Handler;
/// # impl Extension for Handler {
/// #     fn probe(&self) -> usize { 1 }
/// #     fn handle(&self, _: usize, _: [usize; 6]) -> SbiRet { SbiRet::success(0) }
/// # }
/// const EID: usize = 0x0900_031e;
/// #[derive(VendorSBI)]
/// struct Invalid {
///     #[rustsbi(extension(eid = EID))]
///     first: Handler,
///     #[rustsbi(extension(eid = EID))]
///     duplicate: Handler,
/// }
/// ```
pub trait VendorSBI {
    /// Returns the BASE probe value for `extension`; zero means unavailable.
    fn probe_extension(&self, extension: usize) -> usize;

    /// Dispatches one vendor-specific SBI call.
    fn handle_ecall(&self, extension: usize, function: usize, args: [usize; 6]) -> SbiRet;
}

impl<T: VendorSBI + ?Sized> VendorSBI for &T {
    #[inline]
    fn probe_extension(&self, extension: usize) -> usize {
        T::probe_extension(self, extension)
    }

    #[inline]
    fn handle_ecall(&self, extension: usize, function: usize, args: [usize; 6]) -> SbiRet {
        T::handle_ecall(self, extension, function, args)
    }
}

impl<T: VendorSBI> VendorSBI for Option<T> {
    #[inline]
    fn probe_extension(&self, extension: usize) -> usize {
        self.as_ref()
            .map_or(0, |inner| T::probe_extension(inner, extension))
    }

    #[inline]
    fn handle_ecall(&self, extension: usize, function: usize, args: [usize; 6]) -> SbiRet {
        self.as_ref().map_or_else(SbiRet::not_supported, |inner| {
            T::handle_ecall(inner, extension, function, args)
        })
    }
}

impl<T: Extension + ?Sized> Extension for &T {
    #[inline]
    fn probe(&self) -> usize {
        T::probe(self)
    }

    #[inline]
    fn handle(&self, fid: usize, args: [usize; 6]) -> SbiRet {
        T::handle(self, fid, args)
    }
}

impl<T: Extension> Extension for Option<T> {
    #[inline]
    fn probe(&self) -> usize {
        self.as_ref().map_or(0, T::probe)
    }

    #[inline]
    fn handle(&self, fid: usize, args: [usize; 6]) -> SbiRet {
        self.as_ref()
            .map_or_else(SbiRet::not_supported, |inner| T::handle(inner, fid, args))
    }
}
