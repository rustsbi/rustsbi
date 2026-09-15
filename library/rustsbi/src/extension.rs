use crate::SbiRet;

/// A custom SBI extension bound to an EID by [`derive(RustSBI)`](crate::RustSBI).
///
/// Register a field with `#[rustsbi(extension(eid = EID))]`, where `EID` is an integer
/// literal or a module-level `usize` constant path. Each EID may be registered only
/// once and must fit in 32 bits. Standard EIDs, including BASE and legacy calls,
/// cannot be registered as custom extensions, even when their standard backend is absent.
///
/// Both static and dynamic dispatch check availability before calling [`handle`](Self::handle).
/// `Option<T>` makes a field optional, and references forward to their underlying implementation.
///
/// # Invalid registrations
///
/// Duplicate EIDs are rejected, including aliases through constant paths:
///
/// ```compile_fail,E0080
/// # use rustsbi::{Extension, EnvInfo, RustSBI, SbiRet};
/// # struct Handler;
/// # impl Extension for Handler {
/// #     fn probe(&self) -> usize { 1 }
/// #     fn handle(&self, _: usize, _: [usize; 6]) -> SbiRet { SbiRet::success(0) }
/// # }
/// # impl EnvInfo for Handler {
/// #     fn mvendorid(&self) -> usize { 0 }
/// #     fn marchid(&self) -> usize { 0 }
/// #     fn mimpid(&self) -> usize { 0 }
/// # }
/// const EID: usize = 0x0900_031e;
/// const ALIAS: usize = EID;
/// #[derive(RustSBI)]
/// struct Invalid {
///     #[rustsbi(extension(eid = EID))]
///     first: Handler,
///     #[rustsbi(extension(eid = ALIAS))]
///     second: Handler,
///     info: Handler,
/// }
/// ```
///
/// Standard EIDs remain reserved in dynamic mode even without a standard backend:
///
/// ```compile_fail,E0080
/// # use rustsbi::{Extension, EnvInfo, RustSBI, SbiRet};
/// # struct Handler;
/// # impl Extension for Handler {
/// #     fn probe(&self) -> usize { 1 }
/// #     fn handle(&self, _: usize, _: [usize; 6]) -> SbiRet { SbiRet::success(0) }
/// # }
/// # impl EnvInfo for Handler {
/// #     fn mvendorid(&self) -> usize { 0 }
/// #     fn marchid(&self) -> usize { 0 }
/// #     fn mimpid(&self) -> usize { 0 }
/// # }
/// #[derive(RustSBI)]
/// #[rustsbi(dynamic)]
/// struct Invalid {
///     #[rustsbi(extension(eid = rustsbi::spec::time::EID_TIME))]
///     custom: Handler,
///     info: Handler,
/// }
/// ```
///
/// EIDs wider than 32 bits are rejected:
///
/// ```compile_fail
/// # use rustsbi::{Extension, EnvInfo, RustSBI, SbiRet};
/// # struct Handler;
/// # impl Extension for Handler {
/// #     fn probe(&self) -> usize { 1 }
/// #     fn handle(&self, _: usize, _: [usize; 6]) -> SbiRet { SbiRet::success(0) }
/// # }
/// # impl EnvInfo for Handler {
/// #     fn mvendorid(&self) -> usize { 0 }
/// #     fn marchid(&self) -> usize { 0 }
/// #     fn mimpid(&self) -> usize { 0 }
/// # }
/// #[derive(RustSBI)]
/// struct Invalid {
///     #[rustsbi(extension(eid = 0x1_0000_0000))]
///     custom: Handler,
///     info: Handler,
/// }
/// ```
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
