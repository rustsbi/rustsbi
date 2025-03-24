/// SBI functions return type.
///
/// > SBI functions must return a pair of values in a0 and a1,
/// > with a0 returning an error code.
/// > This is analogous to returning the C structure `SbiRet`.
///
/// Note: if this structure is used in function return on conventional
/// Rust code, it would not require pinning memory representation as
/// extern C. The `repr(C)` is set in case that some users want to use
/// this structure in FFI code.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SbiRet<T = usize> {
    /// Error number.
    pub error: T,
    /// Result value.
    pub value: T,
}

/// Standard RISC-V SBI error IDs in `usize`.
pub mod id {
    use super::SbiRegister;

    /// SBI success state return value.
    #[doc(alias = "SBI_SUCCESS")]
    pub const RET_SUCCESS: usize = <usize as SbiRegister>::RET_SUCCESS;
    /// Error for SBI call failed for unknown reasons.
    #[doc(alias = "SBI_ERR_FAILED")]
    pub const RET_ERR_FAILED: usize = <usize as SbiRegister>::RET_ERR_FAILED;
    /// Error for target operation not supported.
    #[doc(alias = "SBI_ERR_NOT_SUPPORTED")]
    pub const RET_ERR_NOT_SUPPORTED: usize = <usize as SbiRegister>::RET_ERR_NOT_SUPPORTED;
    /// Error for invalid parameter.
    #[doc(alias = "SBI_ERR_INVALID_PARAM")]
    pub const RET_ERR_INVALID_PARAM: usize = <usize as SbiRegister>::RET_ERR_INVALID_PARAM;
    /// Error for denied.
    #[doc(alias = "SBI_ERR_DENIED")]
    pub const RET_ERR_DENIED: usize = <usize as SbiRegister>::RET_ERR_DENIED;
    /// Error for invalid address.
    #[doc(alias = "SBI_ERR_INVALID_ADDRESS")]
    pub const RET_ERR_INVALID_ADDRESS: usize = <usize as SbiRegister>::RET_ERR_INVALID_ADDRESS;
    /// Error for resource already available.
    #[doc(alias = "SBI_ERR_ALREADY_AVAILABLE")]
    pub const RET_ERR_ALREADY_AVAILABLE: usize = <usize as SbiRegister>::RET_ERR_ALREADY_AVAILABLE;
    /// Error for resource already started.
    #[doc(alias = "SBI_ERR_ALREADY_STARTED")]
    pub const RET_ERR_ALREADY_STARTED: usize = <usize as SbiRegister>::RET_ERR_ALREADY_STARTED;
    /// Error for resource already stopped.
    #[doc(alias = "SBI_ERR_ALREADY_STOPPED")]
    pub const RET_ERR_ALREADY_STOPPED: usize = <usize as SbiRegister>::RET_ERR_ALREADY_STOPPED;
    /// Error for shared memory not available.
    #[doc(alias = "SBI_ERR_NO_SHMEM")]
    pub const RET_ERR_NO_SHMEM: usize = <usize as SbiRegister>::RET_ERR_NO_SHMEM;
    /// Error for invalid state.
    #[doc(alias = "SBI_ERR_INVALID_STATE")]
    pub const RET_ERR_INVALID_STATE: usize = <usize as SbiRegister>::RET_ERR_INVALID_STATE;
    /// Error for bad or invalid range.
    #[doc(alias = "SBI_ERR_BAD_RANGE")]
    pub const RET_ERR_BAD_RANGE: usize = <usize as SbiRegister>::RET_ERR_BAD_RANGE;
    /// Error for failed due to timeout.
    #[doc(alias = "SBI_ERR_TIMEOUT")]
    pub const RET_ERR_TIMEOUT: usize = <usize as SbiRegister>::RET_ERR_TIMEOUT;
    /// Error for input or output error.
    #[doc(alias = "SBI_ERR_IO")]
    pub const RET_ERR_IO: usize = <usize as SbiRegister>::RET_ERR_IO;
    /// Error for denied or not allowed due to lock status.
    #[doc(alias = "SBI_ERR_DENIED_LOCKED")]
    pub const RET_ERR_DENIED_LOCKED: usize = <usize as SbiRegister>::RET_ERR_DENIED_LOCKED;
    // ^^ Note: remember to add a test case in `rustsbi_sbi_ret_constructors` in this file,
    // and `test_binary` in lib.rs after adding an error number!
}
// Use each constants in `id` module, so that any `match` operations will not treat constant
// names (`RET_ERR_*`) as newly defined variable names.
use id::*;

/// Data type of register that can be passed to the RISC-V SBI ABI.
///
/// This trait defines the requirements for types that are used as the underlying
/// representation for both the `value` and `error` fields in the `SbiRet` structure.
/// In most cases, this trait is implemented for primitive integer types (e.g., `usize`),
/// but it can also be implemented for other types that satisfy the constraints.
///
/// # Examples
///
/// Implemented automatically for all types that satisfy `Copy`, `Eq`, and `Debug`.
pub trait SbiRegister: Copy + Eq + Ord + core::fmt::Debug {
    /// SBI success state return value.
    const RET_SUCCESS: Self;
    /// Error for SBI call failed for unknown reasons.
    const RET_ERR_FAILED: Self;
    /// Error for target operation not supported.
    const RET_ERR_NOT_SUPPORTED: Self;
    /// Error for invalid parameter.
    const RET_ERR_INVALID_PARAM: Self;
    /// Error for denied.
    const RET_ERR_DENIED: Self;
    /// Error for invalid address.
    const RET_ERR_INVALID_ADDRESS: Self;
    /// Error for resource already available.
    const RET_ERR_ALREADY_AVAILABLE: Self;
    /// Error for resource already started.
    const RET_ERR_ALREADY_STARTED: Self;
    /// Error for resource already stopped.
    const RET_ERR_ALREADY_STOPPED: Self;
    /// Error for shared memory not available.
    const RET_ERR_NO_SHMEM: Self;
    /// Error for invalid state.
    const RET_ERR_INVALID_STATE: Self;
    /// Error for bad or invalid range.
    const RET_ERR_BAD_RANGE: Self;
    /// Error for failed due to timeout.
    const RET_ERR_TIMEOUT: Self;
    /// Error for input or output error.
    const RET_ERR_IO: Self;
    /// Error for denied or not allowed due to lock status.
    const RET_ERR_DENIED_LOCKED: Self;

    /// Zero value for this type; this is used on `value` fields once `SbiRet` returns an error.
    const ZERO: Self;
    /// Full-ones value for this type; this is used on SBI mask structures like `CounterMask`
    /// and `HartMask`.
    const FULL_MASK: Self;

    /// Converts an `SbiRet` of this type to a `Result` of self and `Error`.
    fn into_result(ret: SbiRet<Self>) -> Result<Self, Error<Self>>;
}

macro_rules! impl_sbi_register {
    ($ty:ty, $signed:ty) => {
        impl SbiRegister for $ty {
            const RET_SUCCESS: Self = 0;
            const RET_ERR_FAILED: Self = -1 as $signed as $ty;
            const RET_ERR_NOT_SUPPORTED: Self = -2 as $signed as $ty;
            const RET_ERR_INVALID_PARAM: Self = -3 as $signed as $ty;
            const RET_ERR_DENIED: Self = -4 as $signed as $ty;
            const RET_ERR_INVALID_ADDRESS: Self = -5 as $signed as $ty;
            const RET_ERR_ALREADY_AVAILABLE: Self = -6 as $signed as $ty;
            const RET_ERR_ALREADY_STARTED: Self = -7 as $signed as $ty;
            const RET_ERR_ALREADY_STOPPED: Self = -8 as $signed as $ty;
            const RET_ERR_NO_SHMEM: Self = -9 as $signed as $ty;
            const RET_ERR_INVALID_STATE: Self = -10 as $signed as $ty;
            const RET_ERR_BAD_RANGE: Self = -11 as $signed as $ty;
            const RET_ERR_TIMEOUT: Self = -12 as $signed as $ty;
            const RET_ERR_IO: Self = -13 as $signed as $ty;
            const RET_ERR_DENIED_LOCKED: Self = -14 as $signed as $ty;
            const ZERO: Self = 0;
            const FULL_MASK: Self = !0;

            fn into_result(ret: SbiRet<Self>) -> Result<Self, Error<Self>> {
                match ret.error {
                    Self::RET_SUCCESS => Ok(ret.value),
                    Self::RET_ERR_FAILED => Err(Error::Failed),
                    Self::RET_ERR_NOT_SUPPORTED => Err(Error::NotSupported),
                    Self::RET_ERR_INVALID_PARAM => Err(Error::InvalidParam),
                    Self::RET_ERR_DENIED => Err(Error::Denied),
                    Self::RET_ERR_INVALID_ADDRESS => Err(Error::InvalidAddress),
                    Self::RET_ERR_ALREADY_AVAILABLE => Err(Error::AlreadyAvailable),
                    Self::RET_ERR_ALREADY_STARTED => Err(Error::AlreadyStarted),
                    Self::RET_ERR_ALREADY_STOPPED => Err(Error::AlreadyStopped),
                    Self::RET_ERR_NO_SHMEM => Err(Error::NoShmem),
                    Self::RET_ERR_INVALID_STATE => Err(Error::InvalidState),
                    Self::RET_ERR_BAD_RANGE => Err(Error::BadRange),
                    Self::RET_ERR_TIMEOUT => Err(Error::Timeout),
                    Self::RET_ERR_IO => Err(Error::Io),
                    Self::RET_ERR_DENIED_LOCKED => Err(Error::DeniedLocked),
                    unknown => Err(Error::Custom(unknown as _)),
                }
            }
        }
    };
}

impl_sbi_register!(usize, isize);
impl_sbi_register!(isize, isize);
impl_sbi_register!(u32, i32);
impl_sbi_register!(i32, i32);
impl_sbi_register!(u64, i64);
impl_sbi_register!(i64, i64);
impl_sbi_register!(u128, i128);
impl_sbi_register!(i128, i128);

impl<T: SbiRegister + core::fmt::LowerHex> core::fmt::Debug for SbiRet<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match T::into_result(*self) {
            Ok(value) => write!(f, "{:?}", value),
            Err(err) => match err {
                Error::Failed => write!(f, "<SBI call failed>"),
                Error::NotSupported => write!(f, "<SBI feature not supported>"),
                Error::InvalidParam => write!(f, "<SBI invalid parameter>"),
                Error::Denied => write!(f, "<SBI denied>"),
                Error::InvalidAddress => write!(f, "<SBI invalid address>"),
                Error::AlreadyAvailable => write!(f, "<SBI already available>"),
                Error::AlreadyStarted => write!(f, "<SBI already started>"),
                Error::AlreadyStopped => write!(f, "<SBI already stopped>"),
                Error::NoShmem => write!(f, "<SBI shared memory not available>"),
                Error::InvalidState => write!(f, "<SBI invalid state>"),
                Error::BadRange => write!(f, "<SBI bad range>"),
                Error::Timeout => write!(f, "<SBI timeout>"),
                Error::Io => write!(f, "<SBI input/output error>"),
                Error::DeniedLocked => write!(f, "<SBI denied due to locked status>"),
                Error::Custom(unknown) => write!(f, "[SBI Unknown error: {:#x}]", unknown),
            },
        }
    }
}

/// RISC-V SBI error in enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error<T = usize> {
    /// Error for SBI call failed for unknown reasons.
    Failed,
    /// Error for target operation not supported.
    NotSupported,
    /// Error for invalid parameter.
    InvalidParam,
    /// Error for denied.
    Denied,
    /// Error for invalid address.
    InvalidAddress,
    /// Error for resource already available.
    AlreadyAvailable,
    /// Error for resource already started.
    AlreadyStarted,
    /// Error for resource already stopped.
    AlreadyStopped,
    /// Error for shared memory not available.
    NoShmem,
    /// Error for invalid state.
    InvalidState,
    /// Error for bad or invalid range.
    BadRange,
    /// Error for failed due to timeout.
    Timeout,
    /// Error for input or output error.
    Io,
    /// Error for denied or not allowed due to lock status.
    DeniedLocked,
    /// Custom error code.
    Custom(T),
}

impl<T: SbiRegister> SbiRet<T> {
    /// Returns success SBI state with given `value`.
    #[inline]
    pub const fn success(value: T) -> Self {
        Self {
            error: T::RET_SUCCESS,
            value,
        }
    }

    /// The SBI call request failed for unknown reasons.
    #[inline]
    pub const fn failed() -> Self {
        Self {
            error: T::RET_ERR_FAILED,
            value: T::ZERO,
        }
    }

    /// SBI call failed due to not supported by target ISA,
    /// operation type not supported,
    /// or target operation type not implemented on purpose.
    #[inline]
    pub const fn not_supported() -> Self {
        Self {
            error: T::RET_ERR_NOT_SUPPORTED,
            value: T::ZERO,
        }
    }

    /// SBI call failed due to invalid hart mask parameter,
    /// invalid target hart id,
    /// invalid operation type,
    /// or invalid resource index.
    #[inline]
    pub const fn invalid_param() -> Self {
        Self {
            error: T::RET_ERR_INVALID_PARAM,
            value: T::ZERO,
        }
    }
    /// SBI call denied for unsatisfied entry criteria, or insufficient access
    /// permission to debug console or CPPC register.
    #[inline]
    pub const fn denied() -> Self {
        Self {
            error: T::RET_ERR_DENIED,
            value: T::ZERO,
        }
    }

    /// SBI call failed for invalid mask start address,
    /// not a valid physical address parameter,
    /// or the target address is prohibited by PMP to run in supervisor mode.
    #[inline]
    pub const fn invalid_address() -> Self {
        Self {
            error: T::RET_ERR_INVALID_ADDRESS,
            value: T::ZERO,
        }
    }

    /// SBI call failed for the target resource is already available,
    /// e.g., the target hart is already started when caller still requests it to start.
    #[inline]
    pub const fn already_available() -> Self {
        Self {
            error: T::RET_ERR_ALREADY_AVAILABLE,
            value: T::ZERO,
        }
    }

    /// SBI call failed for the target resource is already started,
    /// e.g., target performance counter is started.
    #[inline]
    pub const fn already_started() -> Self {
        Self {
            error: T::RET_ERR_ALREADY_STARTED,
            value: T::ZERO,
        }
    }

    /// SBI call failed for the target resource is already stopped,
    /// e.g., target performance counter is stopped.
    #[inline]
    pub const fn already_stopped() -> Self {
        Self {
            error: T::RET_ERR_ALREADY_STOPPED,
            value: T::ZERO,
        }
    }

    /// SBI call failed for shared memory is not available,
    /// e.g. nested acceleration shared memory is not available.
    #[inline]
    pub const fn no_shmem() -> Self {
        Self {
            error: T::RET_ERR_NO_SHMEM,
            value: T::ZERO,
        }
    }

    /// SBI call failed for invalid state,
    /// e.g. register a software event but the event is not in unused state.
    #[inline]
    pub const fn invalid_state() -> Self {
        Self {
            error: T::RET_ERR_INVALID_STATE,
            value: T::ZERO,
        }
    }

    /// SBI call failed for bad or invalid range,
    /// e.g. the software event is not exist in the specified range.
    #[inline]
    pub const fn bad_range() -> Self {
        Self {
            error: T::RET_ERR_BAD_RANGE,
            value: T::ZERO,
        }
    }

    /// SBI call failed for timeout,
    /// e.g. message send timeout.
    #[inline]
    pub const fn timeout() -> Self {
        Self {
            error: T::RET_ERR_TIMEOUT,
            value: T::ZERO,
        }
    }

    /// SBI call failed for input or output error.
    #[inline]
    pub const fn io() -> Self {
        Self {
            error: T::RET_ERR_IO,
            value: T::ZERO,
        }
    }
    /// SBI call failed for denied or not allowed due to lock status.
    #[inline]
    pub const fn denied_locked() -> Self {
        Self {
            error: T::RET_ERR_DENIED_LOCKED,
            value: T::ZERO,
        }
    }
}

impl<T: SbiRegister> From<Error<T>> for SbiRet<T> {
    #[inline]
    fn from(value: Error<T>) -> Self {
        match value {
            Error::Failed => SbiRet::failed(),
            Error::NotSupported => SbiRet::not_supported(),
            Error::InvalidParam => SbiRet::invalid_param(),
            Error::Denied => SbiRet::denied(),
            Error::InvalidAddress => SbiRet::invalid_address(),
            Error::AlreadyAvailable => SbiRet::already_available(),
            Error::AlreadyStarted => SbiRet::already_started(),
            Error::AlreadyStopped => SbiRet::already_stopped(),
            Error::NoShmem => SbiRet::no_shmem(),
            Error::InvalidState => SbiRet::invalid_state(),
            Error::BadRange => SbiRet::bad_range(),
            Error::Timeout => SbiRet::timeout(),
            Error::Io => SbiRet::io(),
            Error::DeniedLocked => SbiRet::denied_locked(),
            Error::Custom(error) => SbiRet {
                error,
                value: T::ZERO,
            },
        }
    }
}

impl SbiRet {
    /// Converts to a [`Result`] of value and error.
    #[inline]
    pub const fn into_result(self) -> Result<usize, Error> {
        match self.error {
            RET_SUCCESS => Ok(self.value),
            RET_ERR_FAILED => Err(Error::Failed),
            RET_ERR_NOT_SUPPORTED => Err(Error::NotSupported),
            RET_ERR_INVALID_PARAM => Err(Error::InvalidParam),
            RET_ERR_DENIED => Err(Error::Denied),
            RET_ERR_INVALID_ADDRESS => Err(Error::InvalidAddress),
            RET_ERR_ALREADY_AVAILABLE => Err(Error::AlreadyAvailable),
            RET_ERR_ALREADY_STARTED => Err(Error::AlreadyStarted),
            RET_ERR_ALREADY_STOPPED => Err(Error::AlreadyStopped),
            RET_ERR_NO_SHMEM => Err(Error::NoShmem),
            RET_ERR_INVALID_STATE => Err(Error::InvalidState),
            RET_ERR_BAD_RANGE => Err(Error::BadRange),
            RET_ERR_TIMEOUT => Err(Error::Timeout),
            RET_ERR_IO => Err(Error::Io),
            RET_ERR_DENIED_LOCKED => Err(Error::DeniedLocked),
            unknown => Err(Error::Custom(unknown as _)),
        }
    }

    /// Returns `true` if current SBI return succeeded.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use sbi_spec::binary::SbiRet;
    /// let x = SbiRet::success(0);
    /// assert_eq!(x.is_ok(), true);
    ///
    /// let x = SbiRet::failed();
    /// assert_eq!(x.is_ok(), false);
    /// ```
    #[must_use = "if you intended to assert that this is ok, consider `.unwrap()` instead"]
    #[inline]
    pub const fn is_ok(&self) -> bool {
        matches!(self.error, RET_SUCCESS)
    }

    /// Returns `true` if the SBI call succeeded and the value inside of it matches a predicate.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use sbi_spec::binary::SbiRet;
    /// let x = SbiRet::success(2);
    /// assert_eq!(x.is_ok_and(|x| x > 1), true);
    ///
    /// let x = SbiRet::success(0);
    /// assert_eq!(x.is_ok_and(|x| x > 1), false);
    ///
    /// let x = SbiRet::no_shmem();
    /// assert_eq!(x.is_ok_and(|x| x > 1), false);
    /// ```
    #[must_use]
    #[inline]
    pub fn is_ok_and(self, f: impl FnOnce(usize) -> bool) -> bool {
        self.into_result().is_ok_and(f)
    }

    /// Returns `true` if current SBI return is an error.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use sbi_spec::binary::SbiRet;
    /// let x = SbiRet::success(0);
    /// assert_eq!(x.is_err(), false);
    ///
    /// let x = SbiRet::not_supported();
    /// assert_eq!(x.is_err(), true);
    /// ```
    #[must_use = "if you intended to assert that this is err, consider `.unwrap_err()` instead"]
    #[inline]
    pub const fn is_err(&self) -> bool {
        !self.is_ok()
    }

    /// Returns `true` if the result is an error and the value inside of it matches a predicate.
    ///
    /// # Examples
    ///
    /// ```
    /// # use sbi_spec::binary::{SbiRet, Error};
    /// let x = SbiRet::denied();
    /// assert_eq!(x.is_err_and(|x| x == Error::Denied), true);
    ///
    /// let x = SbiRet::invalid_address();
    /// assert_eq!(x.is_err_and(|x| x == Error::Denied), false);
    ///
    /// let x = SbiRet::success(0);
    /// assert_eq!(x.is_err_and(|x| x == Error::Denied), false);
    /// ```
    #[must_use]
    #[inline]
    pub fn is_err_and(self, f: impl FnOnce(Error) -> bool) -> bool {
        self.into_result().is_err_and(f)
    }

    /// Converts from `SbiRet` to [`Option<usize>`].
    ///
    /// Converts `self` into an [`Option<usize>`], consuming `self`,
    /// and discarding the error, if any.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use sbi_spec::binary::SbiRet;
    /// let x = SbiRet::success(2);
    /// assert_eq!(x.ok(), Some(2));
    ///
    /// let x = SbiRet::invalid_param();
    /// assert_eq!(x.ok(), None);
    /// ```
    // fixme: should be pub const fn once this function in Result is stabilized in constant
    #[inline]
    pub fn ok(self) -> Option<usize> {
        self.into_result().ok()
    }

    /// Converts from `SbiRet` to [`Option<Error>`].
    ///
    /// Converts `self` into an [`Option<Error>`], consuming `self`,
    /// and discarding the success value, if any.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use sbi_spec::binary::{SbiRet, Error};
    /// let x = SbiRet::success(2);
    /// assert_eq!(x.err(), None);
    ///
    /// let x = SbiRet::denied();
    /// assert_eq!(x.err(), Some(Error::Denied));
    /// ```
    // fixme: should be pub const fn once this function in Result is stabilized in constant
    #[inline]
    pub fn err(self) -> Option<Error> {
        self.into_result().err()
    }

    /// Maps a `SbiRet` to `Result<U, Error>` by applying a function to a
    /// contained success value, leaving an error value untouched.
    ///
    /// This function can be used to compose the results of two functions.
    ///
    /// # Examples
    ///
    /// Gets detail of a PMU counter and judge if it is a firmware counter.
    ///
    /// ```
    /// # use sbi_spec::binary::SbiRet;
    /// # use core::mem::size_of;
    /// # mod sbi_rt {
    /// #     use sbi_spec::binary::SbiRet;
    /// #     const TYPE_MASK: usize = 1 << (core::mem::size_of::<usize>() - 1);
    /// #     pub fn pmu_counter_get_info(_: usize) -> SbiRet { SbiRet::success(TYPE_MASK) }
    /// # }
    /// // We assume that counter index 42 is a firmware counter.
    /// let counter_idx = 42;
    /// // Masks PMU counter type by setting highest bit in `usize`.
    /// const TYPE_MASK: usize = 1 << (size_of::<usize>() - 1);
    /// // Highest bit of returned `counter_info` represents whether it's
    /// // a firmware counter or a hardware counter.
    /// let is_firmware_counter = sbi_rt::pmu_counter_get_info(counter_idx)
    ///     .map(|counter_info| counter_info & TYPE_MASK != 0);
    /// // If that bit is set, it is a firmware counter.
    /// assert_eq!(is_firmware_counter, Ok(true));
    /// ```
    #[inline]
    pub fn map<U, F: FnOnce(usize) -> U>(self, op: F) -> Result<U, Error> {
        self.into_result().map(op)
    }

    /// Returns the provided default (if error),
    /// or applies a function to the contained value (if success).
    ///
    /// Arguments passed to `map_or` are eagerly evaluated;
    /// if you are passing the result of a function call,
    /// it is recommended to use [`map_or_else`],
    /// which is lazily evaluated.
    ///
    /// [`map_or_else`]: SbiRet::map_or_else
    ///
    /// # Examples
    ///
    /// ```
    /// # use sbi_spec::binary::SbiRet;
    /// let x = SbiRet::success(3);
    /// assert_eq!(x.map_or(42, |v| v & 0b1), 1);
    ///
    /// let x = SbiRet::invalid_address();
    /// assert_eq!(x.map_or(42, |v| v & 0b1), 42);
    /// ```
    #[inline]
    pub fn map_or<U, F: FnOnce(usize) -> U>(self, default: U, f: F) -> U {
        self.into_result().map_or(default, f)
    }

    /// Maps a `SbiRet` to `usize` value by applying fallback function `default` to
    /// a contained error, or function `f` to a contained success value.
    ///
    /// This function can be used to unpack a successful result
    /// while handling an error.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use sbi_spec::binary::SbiRet;
    /// let k = 21;
    ///
    /// let x = SbiRet::success(3);
    /// assert_eq!(x.map_or_else(|e| k * 2, |v| v & 0b1), 1);
    ///
    /// let x = SbiRet::already_available();
    /// assert_eq!(x.map_or_else(|e| k * 2, |v| v & 0b1), 42);
    /// ```
    #[inline]
    pub fn map_or_else<U, D: FnOnce(Error) -> U, F: FnOnce(usize) -> U>(
        self,
        default: D,
        f: F,
    ) -> U {
        self.into_result().map_or_else(default, f)
    }

    /// Maps a `SbiRet` to `Result<T, F>` by applying a function to a
    /// contained error as [`Error`] struct, leaving success value untouched.
    ///
    /// This function can be used to pass through a successful result while handling
    /// an error.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use sbi_spec::binary::{SbiRet, Error};
    /// fn stringify(x: Error) -> String {
    ///     if x == Error::AlreadyStarted {
    ///         "error: already started!".to_string()
    ///     } else {
    ///         "error: other error!".to_string()
    ///     }
    /// }
    ///
    /// let x = SbiRet::success(2);
    /// assert_eq!(x.map_err(stringify), Ok(2));
    ///
    /// let x = SbiRet::already_started();
    /// assert_eq!(x.map_err(stringify), Err("error: already started!".to_string()));
    /// ```
    #[inline]
    pub fn map_err<F, O: FnOnce(Error) -> F>(self, op: O) -> Result<usize, F> {
        self.into_result().map_err(op)
    }

    /// Calls a function with a reference to the contained value if current SBI call succeeded.
    ///
    /// Returns the original result.
    ///
    /// # Examples
    ///
    /// ```
    /// # use sbi_spec::binary::SbiRet;
    /// // Assume that SBI debug console have read 512 bytes into a buffer.
    /// let ret = SbiRet::success(512);
    /// // Inspect the SBI DBCN call result.
    /// let idx = ret
    ///     .inspect(|x| println!("bytes written: {x}"))
    ///     .map(|x| x - 1)
    ///     .expect("SBI DBCN call failed");
    /// assert_eq!(idx, 511);
    /// ```
    #[inline]
    pub fn inspect<F: FnOnce(&usize)>(self, f: F) -> Self {
        if let Ok(ref t) = self.into_result() {
            f(t);
        }

        self
    }

    /// Calls a function with a reference to the contained value if current SBI result is an error.
    ///
    /// Returns the original result.
    ///
    /// # Examples
    ///
    /// ```
    /// # use sbi_spec::binary::SbiRet;
    /// // Assume that SBI debug console write operation failed for invalid parameter.
    /// let ret = SbiRet::invalid_param();
    /// // Print the error if SBI DBCN call failed.
    /// let ret = ret.inspect_err(|e| eprintln!("failed to read from SBI console: {e:?}"));
    /// ```
    #[inline]
    pub fn inspect_err<F: FnOnce(&Error)>(self, f: F) -> Self {
        if let Err(ref e) = self.into_result() {
            f(e);
        }

        self
    }

    // TODO: pub fn iter(&self) -> Iter
    // TODO: pub fn iter_mut(&mut self) -> IterMut

    /// Returns the contained success value, consuming the `self` value.
    ///
    /// # Panics
    ///
    /// Panics if self is an SBI error with a panic message including the
    /// passed message, and the content of the SBI state.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```should_panic
    /// # use sbi_spec::binary::SbiRet;
    /// let x = SbiRet::already_stopped();
    /// x.expect("Testing expect"); // panics with `Testing expect`
    /// ```
    #[inline]
    pub fn expect(self, msg: &str) -> usize {
        self.into_result().expect(msg)
    }

    /// Returns the contained success value, consuming the `self` value.
    ///
    /// # Panics
    ///
    /// Panics if self is an SBI error, with a panic message provided by the
    /// SBI error converted into [`Error`] struct.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use sbi_spec::binary::SbiRet;
    /// let x = SbiRet::success(2);
    /// assert_eq!(x.unwrap(), 2);
    /// ```
    ///
    /// ```should_panic
    /// # use sbi_spec::binary::SbiRet;
    /// let x = SbiRet::failed();
    /// x.unwrap(); // panics
    /// ```
    #[inline]
    pub fn unwrap(self) -> usize {
        self.into_result().unwrap()
    }

    // Note: No unwrap_or_default as we cannot determine a meaningful default value for a successful SbiRet.

    /// Returns the contained error as [`Error`] struct, consuming the `self` value.
    ///
    /// # Panics
    ///
    /// Panics if the self is SBI success value, with a panic message
    /// including the passed message, and the content of the success value.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```should_panic
    /// # use sbi_spec::binary::SbiRet;
    /// let x = SbiRet::success(10);
    /// x.expect_err("Testing expect_err"); // panics with `Testing expect_err`
    /// ```
    #[inline]
    pub fn expect_err(self, msg: &str) -> Error {
        self.into_result().expect_err(msg)
    }

    /// Returns the contained error as [`Error`] struct, consuming the `self` value.
    ///
    /// # Panics
    ///
    /// Panics if the self is SBI success value, with a custom panic message provided
    /// by the success value.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// # use sbi_spec::binary::SbiRet;
    /// let x = SbiRet::success(2);
    /// x.unwrap_err(); // panics with `2`
    /// ```
    ///
    /// ```
    /// # use sbi_spec::binary::{SbiRet, Error};
    /// let x = SbiRet::not_supported();
    /// assert_eq!(x.unwrap_err(), Error::NotSupported);
    /// ```
    #[inline]
    pub fn unwrap_err(self) -> Error {
        self.into_result().unwrap_err()
    }

    // TODO: pub fn into_ok(self) -> usize and pub fn into_err(self) -> Error
    // once `unwrap_infallible` is stabilized

    /// Returns `res` if self is success value, otherwise otherwise returns the contained error
    /// of `self` as [`Error`] struct.
    ///
    /// Arguments passed to `and` are eagerly evaluated; if you are passing the
    /// result of a function call, it is recommended to use [`and_then`], which is
    /// lazily evaluated.
    ///
    /// [`and_then`]: SbiRet::and_then
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use sbi_spec::binary::{SbiRet, Error};
    /// let x = SbiRet::success(2);
    /// let y = SbiRet::invalid_param().into_result();
    /// assert_eq!(x.and(y), Err(Error::InvalidParam));
    ///
    /// let x = SbiRet::denied();
    /// let y = SbiRet::success(3).into_result();
    /// assert_eq!(x.and(y), Err(Error::Denied));
    ///
    /// let x = SbiRet::invalid_address();
    /// let y = SbiRet::already_available().into_result();
    /// assert_eq!(x.and(y), Err(Error::InvalidAddress));
    ///
    /// let x = SbiRet::success(4);
    /// let y = SbiRet::success(5).into_result();
    /// assert_eq!(x.and(y), Ok(5));
    /// ```
    // fixme: should be pub const fn once this function in Result is stabilized in constant
    // fixme: should parameter be `res: SbiRet`?
    #[inline]
    pub fn and<U>(self, res: Result<U, Error>) -> Result<U, Error> {
        self.into_result().and(res)
    }

    /// Calls `op` if self is success value, otherwise returns the contained error
    /// as [`Error`] struct.
    ///
    /// This function can be used for control flow based on `SbiRet` values.
    ///
    /// # Examples
    ///
    /// ```
    /// # use sbi_spec::binary::{SbiRet, Error};
    /// fn sq_then_to_string(x: usize) -> Result<String, Error> {
    ///     x.checked_mul(x).map(|sq| sq.to_string()).ok_or(Error::Failed)
    /// }
    ///
    /// assert_eq!(SbiRet::success(2).and_then(sq_then_to_string), Ok(4.to_string()));
    /// assert_eq!(SbiRet::success(1_000_000_000_000).and_then(sq_then_to_string), Err(Error::Failed));
    /// assert_eq!(SbiRet::invalid_param().and_then(sq_then_to_string), Err(Error::InvalidParam));
    /// ```
    #[inline]
    pub fn and_then<U, F: FnOnce(usize) -> Result<U, Error>>(self, op: F) -> Result<U, Error> {
        self.into_result().and_then(op)
    }

    /// Returns `res` if self is SBI error, otherwise returns the success value of `self`.
    ///
    /// Arguments passed to `or` are eagerly evaluated; if you are passing the
    /// result of a function call, it is recommended to use [`or_else`], which is
    /// lazily evaluated.
    ///
    /// [`or_else`]: Result::or_else
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use sbi_spec::binary::{SbiRet, Error};
    /// let x = SbiRet::success(2);
    /// let y = SbiRet::invalid_param().into_result();
    /// assert_eq!(x.or(y), Ok(2));
    ///
    /// let x = SbiRet::denied();
    /// let y = SbiRet::success(3).into_result();
    /// assert_eq!(x.or(y), Ok(3));
    ///
    /// let x = SbiRet::invalid_address();
    /// let y = SbiRet::already_available().into_result();
    /// assert_eq!(x.or(y), Err(Error::AlreadyAvailable));
    ///
    /// let x = SbiRet::success(4);
    /// let y = SbiRet::success(100).into_result();
    /// assert_eq!(x.or(y), Ok(4));
    /// ```
    // fixme: should be pub const fn once this function in Result is stabilized in constant
    // fixme: should parameter be `res: SbiRet`?
    #[inline]
    pub fn or<F>(self, res: Result<usize, F>) -> Result<usize, F> {
        self.into_result().or(res)
    }

    /// Calls `op` if self is SBI error, otherwise returns the success value of `self`.
    ///
    /// This function can be used for control flow based on result values.
    ///
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use sbi_spec::binary::{SbiRet, Error};
    /// fn is_failed(x: Error) -> Result<usize, bool> { Err(x == Error::Failed) }
    ///
    /// assert_eq!(SbiRet::success(2).or_else(is_failed), Ok(2));
    /// assert_eq!(SbiRet::failed().or_else(is_failed), Err(true));
    /// ```
    #[inline]
    pub fn or_else<F, O: FnOnce(Error) -> Result<usize, F>>(self, op: O) -> Result<usize, F> {
        self.into_result().or_else(op)
    }

    /// Returns the contained success value or a provided default.
    ///
    /// Arguments passed to `unwrap_or` are eagerly evaluated; if you are passing
    /// the result of a function call, it is recommended to use [`unwrap_or_else`],
    /// which is lazily evaluated.
    ///
    /// [`unwrap_or_else`]: SbiRet::unwrap_or_else
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use sbi_spec::binary::SbiRet;
    /// let default = 2;
    /// let x = SbiRet::success(9);
    /// assert_eq!(x.unwrap_or(default), 9);
    ///
    /// let x = SbiRet::invalid_param();
    /// assert_eq!(x.unwrap_or(default), default);
    /// ```
    // fixme: should be pub const fn once this function in Result is stabilized in constant
    #[inline]
    pub fn unwrap_or(self, default: usize) -> usize {
        self.into_result().unwrap_or(default)
    }

    /// Returns the contained success value or computes it from a closure.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use sbi_spec::binary::{SbiRet, Error};
    /// fn invalid_use_zero(x: Error) -> usize { if x == Error::InvalidParam { 0 } else { 3 } }
    ///
    /// assert_eq!(SbiRet::success(2).unwrap_or_else(invalid_use_zero), 2);
    /// assert_eq!(SbiRet::invalid_param().unwrap_or_else(invalid_use_zero), 0);
    /// ```
    #[inline]
    pub fn unwrap_or_else<F: FnOnce(Error) -> usize>(self, op: F) -> usize {
        self.into_result().unwrap_or_else(op)
    }

    /// Returns the contained success value, consuming the `self` value,
    /// without checking that the `SbiRet` contains an error value.
    ///
    /// # Safety
    ///
    /// Calling this method on an `SbiRet` containing an error value results
    /// in *undefined behavior*.
    ///
    /// # Examples
    ///
    /// ```
    /// # use sbi_spec::binary::{SbiRet, Error};
    /// let x = SbiRet::success(3);
    /// assert_eq!(unsafe { x.unwrap_unchecked() }, 3);
    /// ```
    ///
    /// ```no_run
    /// # use sbi_spec::binary::SbiRet;
    /// let x = SbiRet::no_shmem();
    /// unsafe { x.unwrap_unchecked(); } // Undefined behavior!
    /// ```
    #[inline]
    pub unsafe fn unwrap_unchecked(self) -> usize {
        unsafe { self.into_result().unwrap_unchecked() }
    }

    /// Returns the contained `Error` value, consuming the `self` value,
    /// without checking that the `SbiRet` does not contain a success value.
    ///
    /// # Safety
    ///
    /// Calling this method on an `SbiRet` containing a success value results
    /// in *undefined behavior*.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use sbi_spec::binary::SbiRet;
    /// let x = SbiRet::success(4);
    /// unsafe { x.unwrap_unchecked(); } // Undefined behavior!
    /// ```
    ///
    /// ```
    /// # use sbi_spec::binary::{SbiRet, Error};
    /// let x = SbiRet::failed();
    /// assert_eq!(unsafe { x.unwrap_err_unchecked() }, Error::Failed);
    /// ```
    #[inline]
    pub unsafe fn unwrap_err_unchecked(self) -> Error {
        unsafe { self.into_result().unwrap_err_unchecked() }
    }
}

impl IntoIterator for SbiRet {
    type Item = usize;
    type IntoIter = core::result::IntoIter<usize>;

    /// Returns a consuming iterator over the possibly contained value.
    ///
    /// The iterator yields one value if the result contains a success value, otherwise none.
    ///
    /// # Examples
    ///
    /// ```
    /// # use sbi_spec::binary::SbiRet;
    /// let x = SbiRet::success(5);
    /// let v: Vec<usize> = x.into_iter().collect();
    /// assert_eq!(v, [5]);
    ///
    /// let x = SbiRet::not_supported();
    /// let v: Vec<usize> = x.into_iter().collect();
    /// assert_eq!(v, []);
    /// ```
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.into_result().into_iter()
    }
}

// TODO: implement Try and FromResidual for SbiRet once those traits are stabilized
/*
impl core::ops::Try for SbiRet {
    type Output = usize;
    type Residual = Result<core::convert::Infallible, Error>;

    #[inline]
    fn from_output(output: Self::Output) -> Self {
        SbiRet::success(output)
    }

    #[inline]
    fn branch(self) -> core::ops::ControlFlow<Self::Residual, Self::Output> {
        self.into_result().branch()
    }
}

impl core::ops::FromResidual<Result<core::convert::Infallible, Error>> for SbiRet {
    #[inline]
    #[track_caller]
    fn from_residual(residual: Result<core::convert::Infallible, Error>) -> Self {
        match residual {
            Err(e) => e.into(),
        }
    }
}

/// ```
/// # use sbi_spec::binary::SbiRet;
/// fn test() -> SbiRet {
///     let value = SbiRet::failed()?;
///     SbiRet::success(0)
/// }
/// assert_eq!(test(), SbiRet::failed());
/// ```
mod test_try_trait_for_sbiret {}
*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[rustfmt::skip]
    fn rustsbi_sbi_ret_constructors() {
        assert_eq!(SbiRet::success(0), SbiRet { value: 0, error: 0 });
        assert_eq!(SbiRet::success(1037), SbiRet { value: 1037, error: 0 });
        assert_eq!(SbiRet::success(usize::MAX), SbiRet { value: usize::MAX, error: 0 });

        assert_eq!(SbiRet::failed(), SbiRet { value: 0, error: usize::MAX - 1 + 1 });
        assert_eq!(SbiRet::not_supported(), SbiRet { value: 0, error: usize::MAX - 2 + 1 });
        assert_eq!(SbiRet::invalid_param(), SbiRet { value: 0, error: usize::MAX - 3 + 1 });
        assert_eq!(SbiRet::denied(), SbiRet { value: 0, error: usize::MAX - 4 + 1 });
        assert_eq!(SbiRet::invalid_address(), SbiRet { value: 0, error: usize::MAX - 5 + 1 });
        assert_eq!(SbiRet::already_available(), SbiRet { value: 0, error: usize::MAX - 6 + 1 });
        assert_eq!(SbiRet::already_started(), SbiRet { value: 0, error: usize::MAX - 7 + 1 });
        assert_eq!(SbiRet::already_stopped(), SbiRet { value: 0, error: usize::MAX - 8 + 1 });
        assert_eq!(SbiRet::no_shmem(), SbiRet { value: 0, error: usize::MAX - 9 + 1 });
        assert_eq!(SbiRet::invalid_state(), SbiRet { value: 0, error: usize::MAX - 10 + 1 });
        assert_eq!(SbiRet::bad_range(), SbiRet { value: 0, error: usize::MAX - 11 + 1 });
        assert_eq!(SbiRet::timeout(), SbiRet { value: 0, error: usize::MAX - 12 + 1 });
        assert_eq!(SbiRet::io(), SbiRet { value: 0, error: usize::MAX - 13 + 1 });
        assert_eq!(SbiRet::denied_locked(), SbiRet { value: 0, error: usize::MAX - 14 + 1 });
    }
}
