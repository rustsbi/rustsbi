//! Chapter 3. Binary Encoding.

use core::marker::PhantomData;

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
// ^^ Note: remember to add a test case in `rustsbi_sbi_ret_constructors` after adding an error number!

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
    // fixme: should be pub const fn once this function in Result is stablized in constant
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
    // fixme: should be pub const fn once this function in Result is stablized in constant
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
    // once `unwrap_infallible` is stablized

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
    // fixme: should be pub const fn once this function in Result is stablized in constant
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
    // fixme: should be pub const fn once this function in Result is stablized in constant
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
    // fixme: should be pub const fn once this function in Result is stablized in constant
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

// TODO: implement Try and FromResidual for SbiRet once those traits are stablized
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

/// Check if the implementation can contains the provided `bit`.
#[inline]
pub(crate) const fn valid_bit(base: usize, bit: usize) -> bool {
    if bit < base {
        // invalid index, under minimum range.
        false
    } else if (bit - base) >= usize::BITS as usize {
        // invalid index, over max range.
        false
    } else {
        true
    }
}

/// Check if the implementation contains the provided `bit`.
///
/// ## Parameters
///
/// - `mask`: bitmask defining the range of bits.
/// - `base`: the starting bit index. (default: `0`)
/// - `ignore`: if `base` is equal to this value, ignore the `mask` parameter, and consider all `bit`s set.
/// - `bit`: the bit index to check for membership in the `mask`.
#[inline]
pub(crate) const fn has_bit(mask: usize, base: usize, ignore: usize, bit: usize) -> bool {
    if base == ignore {
        // ignore the `mask`, consider all `bit`s as set.
        true
    } else if !valid_bit(base, bit) {
        false
    } else {
        // index is in range, check if it is set in the mask.
        mask & (1 << (bit - base)) != 0
    }
}

/// Hart mask structure in SBI function calls.
#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct HartMask<T = usize> {
    hart_mask: T,
    hart_mask_base: T,
}

impl<T: SbiRegister> HartMask<T> {
    /// Special value to ignore the `mask`, and consider all `bit`s as set.
    pub const IGNORE_MASK: T = T::FULL_MASK;

    /// Construct a [HartMask] from mask value and base hart id.
    #[inline]
    pub const fn from_mask_base(hart_mask: T, hart_mask_base: T) -> Self {
        Self {
            hart_mask,
            hart_mask_base,
        }
    }

    /// Construct a [HartMask] that selects all available harts on the current environment.
    ///
    /// According to the RISC-V SBI Specification, `hart_mask_base` can be set to `-1` (i.e. `usize::MAX`)
    /// to indicate that `hart_mask` shall be ignored and all available harts must be considered.
    /// In case of this function in the `sbi-spec` crate, we fill in `usize::MAX` in `hart_mask_base`
    /// parameter to match the RISC-V SBI standard, while choosing 0 as the ignored `hart_mask` value.
    #[inline]
    pub const fn all() -> Self {
        Self {
            hart_mask: T::ZERO,
            hart_mask_base: T::FULL_MASK,
        }
    }

    /// Gets the special value for ignoring the `mask` parameter.
    #[inline]
    pub const fn ignore_mask(&self) -> T {
        Self::IGNORE_MASK
    }

    /// Returns `mask` and `base` parameters from the [HartMask].
    #[inline]
    pub const fn into_inner(self) -> (T, T) {
        (self.hart_mask, self.hart_mask_base)
    }
}

// FIXME: implement for T: SbiRegister once we can implement this using const traits.
// Ref: https://rust-lang.github.io/rust-project-goals/2024h2/const-traits.html
impl HartMask<usize> {
    /// Returns whether the [HartMask] contains the provided `hart_id`.
    #[inline]
    pub const fn has_bit(self, hart_id: usize) -> bool {
        has_bit(
            self.hart_mask,
            self.hart_mask_base,
            Self::IGNORE_MASK,
            hart_id,
        )
    }

    /// Insert a hart id into this [HartMask].
    ///
    /// Returns error when `hart_id` is invalid.
    #[inline]
    pub const fn insert(&mut self, hart_id: usize) -> Result<(), MaskError> {
        if self.hart_mask_base == Self::IGNORE_MASK {
            Ok(())
        } else if valid_bit(self.hart_mask_base, hart_id) {
            self.hart_mask |= 1usize << (hart_id - self.hart_mask_base);
            Ok(())
        } else {
            Err(MaskError::InvalidBit)
        }
    }

    /// Remove a hart id from this [HartMask].
    ///
    /// Returns error when `hart_id` is invalid, or it has been ignored.
    #[inline]
    pub const fn remove(&mut self, hart_id: usize) -> Result<(), MaskError> {
        if self.hart_mask_base == Self::IGNORE_MASK {
            Err(MaskError::Ignored)
        } else if valid_bit(self.hart_mask_base, hart_id) {
            self.hart_mask &= !(1usize << (hart_id - self.hart_mask_base));
            Ok(())
        } else {
            Err(MaskError::InvalidBit)
        }
    }

    /// Returns [HartIds] of self.
    #[inline]
    pub const fn iter(&self) -> HartIds {
        HartIds {
            inner: match self.hart_mask_base {
                Self::IGNORE_MASK => UnvisitedMask::Range(0, usize::MAX),
                _ => UnvisitedMask::MaskBase(self.hart_mask, self.hart_mask_base),
            },
        }
    }
}

impl IntoIterator for HartMask {
    type Item = usize;

    type IntoIter = HartIds;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterator structure for `HartMask`.
///
/// It will iterate hart id from low to high.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct HartIds {
    inner: UnvisitedMask,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
enum UnvisitedMask {
    MaskBase(usize, usize),
    Range(usize, usize),
}

impl Iterator for HartIds {
    type Item = usize;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            UnvisitedMask::MaskBase(0, _base) => None,
            UnvisitedMask::MaskBase(unvisited_mask, base) => {
                let low_bit = unvisited_mask.trailing_zeros();
                let hart_id = usize::try_from(low_bit).unwrap() + *base;
                *unvisited_mask &= !(1usize << low_bit);
                Some(hart_id)
            }
            UnvisitedMask::Range(start, end) => {
                assert!(start <= end);
                if *start < *end {
                    let ans = *start;
                    *start += 1;
                    Some(ans)
                } else {
                    None
                }
            }
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self.inner {
            UnvisitedMask::MaskBase(unvisited_mask, _base) => {
                let exact_popcnt = usize::try_from(unvisited_mask.count_ones()).unwrap();
                (exact_popcnt, Some(exact_popcnt))
            }
            UnvisitedMask::Range(start, end) => {
                assert!(start <= end);
                let exact_num_harts = end - start;
                (exact_num_harts, Some(exact_num_harts))
            }
        }
    }

    #[inline]
    fn count(self) -> usize {
        self.size_hint().0
    }

    #[inline]
    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }

    #[inline]
    fn min(mut self) -> Option<Self::Item> {
        self.next()
    }

    #[inline]
    fn max(mut self) -> Option<Self::Item> {
        self.next_back()
    }

    #[inline]
    fn is_sorted(self) -> bool {
        true
    }

    // TODO: implement fn advance_by once it's stablized: https://github.com/rust-lang/rust/issues/77404
    // #[inline]
    // fn advance_by(&mut self, n: usize) -> Result<(), core::num::NonZero<usize>> { ... }
}

impl DoubleEndedIterator for HartIds {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            UnvisitedMask::MaskBase(0, _base) => None,
            UnvisitedMask::MaskBase(unvisited_mask, base) => {
                let high_bit = unvisited_mask.leading_zeros();
                let hart_id = usize::try_from(usize::BITS - high_bit - 1).unwrap() + *base;
                *unvisited_mask &= !(1usize << (usize::BITS - high_bit - 1));
                Some(hart_id)
            }
            UnvisitedMask::Range(start, end) => {
                assert!(start <= end);
                if *start < *end {
                    let ans = *end;
                    *end -= 1;
                    Some(ans)
                } else {
                    None
                }
            }
        }
    }

    // TODO: implement advance_back_by once stablized.
    // #[inline]
    // fn advance_back_by(&mut self, n: usize) -> Result<(), core::num::NonZero<usize>> { ... }
}

impl ExactSizeIterator for HartIds {}

impl core::iter::FusedIterator for HartIds {}

/// Error of mask modification.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum MaskError {
    /// This mask has been ignored.
    Ignored,
    /// Request bit is invalid.
    InvalidBit,
}

/// Counter index mask structure in SBI function calls for the `PMU` extension §11.
#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct CounterMask<T = usize> {
    counter_idx_mask: T,
    counter_idx_base: T,
}

impl<T: SbiRegister> CounterMask<T> {
    /// Special value to ignore the `mask`, and consider all `bit`s as set.
    pub const IGNORE_MASK: T = T::FULL_MASK;

    /// Construct a [CounterMask] from mask value and base counter index.
    #[inline]
    pub const fn from_mask_base(counter_idx_mask: T, counter_idx_base: T) -> Self {
        Self {
            counter_idx_mask,
            counter_idx_base,
        }
    }

    /// Gets the special value for ignoring the `mask` parameter.
    #[inline]
    pub const fn ignore_mask(&self) -> T {
        Self::IGNORE_MASK
    }

    /// Returns `mask` and `base` parameters from the [CounterMask].
    #[inline]
    pub const fn into_inner(self) -> (T, T) {
        (self.counter_idx_mask, self.counter_idx_base)
    }
}

// FIXME: implement for T: SbiRegister once we can implement this using const traits.
// Ref: https://rust-lang.github.io/rust-project-goals/2024h2/const-traits.html
impl CounterMask<usize> {
    /// Returns whether the [CounterMask] contains the provided `counter`.
    #[inline]
    pub const fn has_bit(self, counter: usize) -> bool {
        has_bit(
            self.counter_idx_mask,
            self.counter_idx_base,
            Self::IGNORE_MASK,
            counter,
        )
    }
}

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

/// Physical slice wrapper with type annotation.
///
/// This struct wraps slices in RISC-V physical memory by low and high part of the
/// physical base address as well as its length. It is usually used by SBI extensions
/// as parameter types to pass base address and length parameters on physical memory
/// other than a virtual one.
///
/// Generic parameter `P` represents a hint of how this physical slice would be used.
/// For example, `Physical<&[u8]>` represents an immutable reference to physical byte slice,
/// while `Physical<&mut [u8]>` represents a mutable one.
///
/// An SBI implementation should load or store memory using both `phys_addr_lo` and
/// `phys_addr_hi` combined as base address. A supervisor program (kernels etc.)
/// should provide continuous physical memory, wrapping its reference using this structure
/// before passing into SBI runtime.
#[derive(Clone, Copy)]
pub struct Physical<P> {
    num_bytes: usize,
    phys_addr_lo: usize,
    phys_addr_hi: usize,
    _marker: PhantomData<P>,
}

impl<P> Physical<P> {
    /// Create a physical memory slice by length and physical address.
    #[inline]
    pub const fn new(num_bytes: usize, phys_addr_lo: usize, phys_addr_hi: usize) -> Self {
        Self {
            num_bytes,
            phys_addr_lo,
            phys_addr_hi,
            _marker: core::marker::PhantomData,
        }
    }

    /// Returns length of the physical memory slice.
    #[inline]
    pub const fn num_bytes(&self) -> usize {
        self.num_bytes
    }

    /// Returns low-part base address of physical memory slice.
    #[inline]
    pub const fn phys_addr_lo(&self) -> usize {
        self.phys_addr_lo
    }

    /// Returns high-part base address of physical memory slice.
    #[inline]
    pub const fn phys_addr_hi(&self) -> usize {
        self.phys_addr_hi
    }
}

/// Shared memory physical address raw pointer with type annotation.
///
/// This is a structure wrapping a raw pointer to the value of the type `T` without
/// a pointer metadata. `SharedPtr`'s are _thin_; they won't include metadata
/// as RISC-V SBI does not provide an approach to pass them via SBI calls,
/// thus the length of type `T` should be decided independently of raw
/// pointer structure.
///
/// `SharedPtr` can be used as a parameter to pass the shared memory physical pointer
///  with a given base address in RISC-V SBI calls. For example, a `SharedPtr<[u8; 64]>`
/// would represent a fixed-size 64 byte array on a RISC-V SBI function argument
/// type.
///
/// This structure cannot be dereferenced directly with physical addresses,
/// because on RISC-V systems the physical address space could be larger than the
/// virtual ones. Hence, this structure describes the physical memory range by
/// two `usize` values: the upper `phys_addr_hi` and lower `phys_addr_lo`.
///
/// RISC-V SBI extensions may declare special pointer values for shared memory
/// raw pointers. For example, SBI STA declares that steal-time information
/// should stop from reporting when the SBI call is invoked using all-ones
/// bitwise shared pointer, i.e. `phys_addr_hi` and `phys_addr_lo` both equals
/// `usize::MAX`. `SharedPtr` can be constructed using such special values
/// by providing them to the `SharedPtr::new` function.
///
/// # Requirements
///
/// If an SBI function needs to pass a shared memory physical address range to
/// the SBI implementation (or higher privilege mode), then this physical memory
/// address range MUST satisfy the following requirements:
///
/// * The SBI implementation MUST check that the supervisor-mode software is
///   allowed to access the specified physical memory range with the access
///   type requested (read and/or write).
/// * The SBI implementation MUST access the specified physical memory range
///   using the PMA attributes.
/// * The data in the shared memory MUST follow little-endian byte ordering.
///
/// *NOTE:* If the supervisor-mode software accesses the same physical memory
/// range using a memory type different from the PMA, then a loss of coherence
/// or unexpected memory ordering may occur. The invoking software should
/// follow the rules and sequences defined in the RISC-V Svpbmt specification
/// to prevent the loss of coherence and memory ordering.
///
/// It is recommended that a memory physical address passed to an SBI function
/// should use at least two `usize` parameters to support platforms
/// which have memory physical addresses wider than `XLEN` bits.
// FIXME: should constrain with `T: Thin` once ptr_metadata feature is stabled;
// RISC-V SBI does not provide an approach to pass pointer metadata by SBI calls.
pub struct SharedPtr<T> {
    phys_addr_lo: usize,
    phys_addr_hi: usize,
    _marker: PhantomData<*mut T>,
}

// FIXME: we should consider strict provenance rules for this pointer-like structure
// once feature strict_provenance is stabled.
impl<T> SharedPtr<T> {
    /// Create a shared physical memory pointer by physical address.
    #[inline]
    pub const fn new(phys_addr_lo: usize, phys_addr_hi: usize) -> Self {
        Self {
            phys_addr_lo,
            phys_addr_hi,
            _marker: PhantomData,
        }
    }

    /// Returns low-part physical address of the shared physical memory pointer.
    #[inline]
    pub const fn phys_addr_lo(self) -> usize {
        self.phys_addr_lo
    }

    /// Returns high-part physical address of the shared physical memory pointer.
    #[inline]
    pub const fn phys_addr_hi(self) -> usize {
        self.phys_addr_hi
    }
}

impl<T> Clone for SharedPtr<T> {
    #[inline(always)]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for SharedPtr<T> {}

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

    #[test]
    fn rustsbi_hart_mask() {
        let mask = HartMask::from_mask_base(0b1, 400);
        assert!(!mask.has_bit(0));
        assert!(mask.has_bit(400));
        assert!(!mask.has_bit(401));
        let mask = HartMask::from_mask_base(0b110, 500);
        assert!(!mask.has_bit(0));
        assert!(!mask.has_bit(500));
        assert!(mask.has_bit(501));
        assert!(mask.has_bit(502));
        assert!(!mask.has_bit(500 + (usize::BITS as usize)));
        let max_bit = 1 << (usize::BITS - 1);
        let mask = HartMask::from_mask_base(max_bit, 600);
        assert!(mask.has_bit(600 + (usize::BITS as usize) - 1));
        assert!(!mask.has_bit(600 + (usize::BITS as usize)));
        let mask = HartMask::from_mask_base(0b11, usize::MAX - 1);
        assert!(!mask.has_bit(usize::MAX - 2));
        assert!(mask.has_bit(usize::MAX - 1));
        assert!(mask.has_bit(usize::MAX));
        assert!(!mask.has_bit(0));
        // hart_mask_base == usize::MAX is special, it means hart_mask should be ignored
        // and this hart mask contains all harts available
        let mask = HartMask::from_mask_base(0, usize::MAX);
        for i in 0..5 {
            assert!(mask.has_bit(i));
        }
        assert!(mask.has_bit(usize::MAX));

        let mut mask = HartMask::from_mask_base(0, 1);
        assert!(!mask.has_bit(1));
        assert!(mask.insert(1).is_ok());
        assert!(mask.has_bit(1));
        assert!(mask.remove(1).is_ok());
        assert!(!mask.has_bit(1));
    }

    #[test]
    fn rustsbi_hart_ids_iterator() {
        let mask = HartMask::from_mask_base(0b101011, 1);
        // Test the `next` method of `HartIds` structure.
        let mut hart_ids = mask.iter();
        assert_eq!(hart_ids.next(), Some(1));
        assert_eq!(hart_ids.next(), Some(2));
        assert_eq!(hart_ids.next(), Some(4));
        assert_eq!(hart_ids.next(), Some(6));
        assert_eq!(hart_ids.next(), None);
        // `HartIds` structures are fused, meaning they return `None` forever once iteration finished.
        assert_eq!(hart_ids.next(), None);

        // Test `for` loop on mask (`HartMask`) as `IntoIterator`.
        let mut ans = [0; 4];
        let mut idx = 0;
        for hart_id in mask {
            ans[idx] = hart_id;
            idx += 1;
        }
        assert_eq!(ans, [1, 2, 4, 6]);

        // Test `Iterator` methods on `HartIds`.
        let mut hart_ids = mask.iter();
        assert_eq!(hart_ids.size_hint(), (4, Some(4)));
        let _ = hart_ids.next();
        assert_eq!(hart_ids.size_hint(), (3, Some(3)));
        let _ = hart_ids.next();
        let _ = hart_ids.next();
        assert_eq!(hart_ids.size_hint(), (1, Some(1)));
        let _ = hart_ids.next();
        assert_eq!(hart_ids.size_hint(), (0, Some(0)));
        let _ = hart_ids.next();
        assert_eq!(hart_ids.size_hint(), (0, Some(0)));

        let mut hart_ids = mask.iter();
        assert_eq!(hart_ids.count(), 4);
        let _ = hart_ids.next();
        assert_eq!(hart_ids.count(), 3);
        let _ = hart_ids.next();
        let _ = hart_ids.next();
        let _ = hart_ids.next();
        assert_eq!(hart_ids.count(), 0);
        let _ = hart_ids.next();
        assert_eq!(hart_ids.count(), 0);

        let hart_ids = mask.iter();
        assert_eq!(hart_ids.last(), Some(6));

        let mut hart_ids = mask.iter();
        assert_eq!(hart_ids.nth(2), Some(4));
        let mut hart_ids = mask.iter();
        assert_eq!(hart_ids.nth(0), Some(1));

        let mut iter = mask.iter().step_by(2);
        assert_eq!(iter.next(), Some(1));
        assert_eq!(iter.next(), Some(4));
        assert_eq!(iter.next(), None);

        let mask_2 = HartMask::from_mask_base(0b1001101, 64);
        let mut iter = mask.iter().chain(mask_2);
        assert_eq!(iter.next(), Some(1));
        assert_eq!(iter.next(), Some(2));
        assert_eq!(iter.next(), Some(4));
        assert_eq!(iter.next(), Some(6));
        assert_eq!(iter.next(), Some(64));
        assert_eq!(iter.next(), Some(66));
        assert_eq!(iter.next(), Some(67));
        assert_eq!(iter.next(), Some(70));
        assert_eq!(iter.next(), None);

        let mut iter = mask.iter().zip(mask_2);
        assert_eq!(iter.next(), Some((1, 64)));
        assert_eq!(iter.next(), Some((2, 66)));
        assert_eq!(iter.next(), Some((4, 67)));
        assert_eq!(iter.next(), Some((6, 70)));
        assert_eq!(iter.next(), None);

        fn to_plic_context_id(hart_id_machine: usize) -> usize {
            hart_id_machine * 2
        }
        let mut iter = mask.iter().map(to_plic_context_id);
        assert_eq!(iter.next(), Some(2));
        assert_eq!(iter.next(), Some(4));
        assert_eq!(iter.next(), Some(8));
        assert_eq!(iter.next(), Some(12));
        assert_eq!(iter.next(), None);

        let mut channel_received = [0; 4];
        let mut idx = 0;
        let mut channel_send = |hart_id| {
            channel_received[idx] = hart_id;
            idx += 1;
        };
        mask.iter().for_each(|value| channel_send(value));
        assert_eq!(channel_received, [1, 2, 4, 6]);

        let is_in_cluster_1 = |hart_id: &usize| *hart_id >= 4 && *hart_id < 7;
        let mut iter = mask.iter().filter(is_in_cluster_1);
        assert_eq!(iter.next(), Some(4));
        assert_eq!(iter.next(), Some(6));
        assert_eq!(iter.next(), None);

        let if_in_cluster_1_get_plic_context_id = |hart_id: usize| {
            if hart_id >= 4 && hart_id < 7 {
                Some(hart_id * 2)
            } else {
                None
            }
        };
        let mut iter = mask.iter().filter_map(if_in_cluster_1_get_plic_context_id);
        assert_eq!(iter.next(), Some(8));
        assert_eq!(iter.next(), Some(12));
        assert_eq!(iter.next(), None);

        let mut iter = mask.iter().enumerate();
        assert_eq!(iter.next(), Some((0, 1)));
        assert_eq!(iter.next(), Some((1, 2)));
        assert_eq!(iter.next(), Some((2, 4)));
        assert_eq!(iter.next(), Some((3, 6)));
        assert_eq!(iter.next(), None);
        let mut ans = [(0, 0); 4];
        let mut idx = 0;
        for (i, hart_id) in mask.iter().enumerate() {
            ans[idx] = (i, hart_id);
            idx += 1;
        }
        assert_eq!(ans, [(0, 1), (1, 2), (2, 4), (3, 6)]);

        let mut iter = mask.iter().peekable();
        assert_eq!(iter.peek(), Some(&1));
        assert_eq!(iter.next(), Some(1));
        assert_eq!(iter.peek(), Some(&2));
        assert_eq!(iter.next(), Some(2));
        assert_eq!(iter.peek(), Some(&4));
        assert_eq!(iter.next(), Some(4));
        assert_eq!(iter.peek(), Some(&6));
        assert_eq!(iter.next(), Some(6));
        assert_eq!(iter.peek(), None);
        assert_eq!(iter.next(), None);

        // TODO: other iterator tests.

        assert!(mask.iter().is_sorted());
        assert!(mask.iter().is_sorted_by(|a, b| a <= b));

        // Reverse iterator as `DoubleEndedIterator`.
        let mut iter = mask.iter().rev();
        assert_eq!(iter.next(), Some(6));
        assert_eq!(iter.next(), Some(4));
        assert_eq!(iter.next(), Some(2));
        assert_eq!(iter.next(), Some(1));
        assert_eq!(iter.next(), None);

        // Special iterator values.
        let nothing = HartMask::from_mask_base(0, 1000);
        assert!(nothing.iter().eq([]));

        let all_mask_bits_set = HartMask::from_mask_base(usize::MAX, 1000);
        let range = 1000..(1000 + usize::BITS as usize);
        assert!(all_mask_bits_set.iter().eq(range));

        let all_harts = HartMask::all();
        let mut iter = all_harts.iter();
        assert_eq!(iter.size_hint(), (usize::MAX, Some(usize::MAX)));
        // Don't use `Iterator::eq` here; it would literally run `Iterator::try_for_each` from 0 to usize::MAX
        // which will cost us forever to run the test.
        assert_eq!(iter.next(), Some(0));
        assert_eq!(iter.size_hint(), (usize::MAX - 1, Some(usize::MAX - 1)));
        assert_eq!(iter.next(), Some(1));
        assert_eq!(iter.next(), Some(2));
        // skip 500 elements
        let _ = iter.nth(500 - 1);
        assert_eq!(iter.next(), Some(503));
        assert_eq!(iter.size_hint(), (usize::MAX - 504, Some(usize::MAX - 504)));
        assert_eq!(iter.next_back(), Some(usize::MAX));
        assert_eq!(iter.next_back(), Some(usize::MAX - 1));
        assert_eq!(iter.size_hint(), (usize::MAX - 506, Some(usize::MAX - 506)));

        // A common usage of `HartMask::all`, we assume that this platform filters out hart 0..=3.
        let environment_available_hart_ids = 4..128;
        // `hart_mask_iter` contains 64..=usize::MAX.
        let hart_mask_iter = all_harts.iter().skip(64);
        let filtered_iter = environment_available_hart_ids.filter(|&x| {
            hart_mask_iter
                .clone()
                .find(|&y| y >= x)
                .map_or(false, |y| y == x)
        });
        assert!(filtered_iter.eq(64..128));

        // The following operations should have O(1) complexity.
        let all_harts = HartMask::all();
        assert_eq!(all_harts.iter().count(), usize::MAX);
        assert_eq!(all_harts.iter().last(), Some(usize::MAX));
        assert_eq!(all_harts.iter().min(), Some(0));
        assert_eq!(all_harts.iter().max(), Some(usize::MAX));
        assert!(all_harts.iter().is_sorted());

        let partial_all_harts = {
            let mut ans = HartMask::all().iter();
            let _ = ans.nth(65536 - 1);
            let _ = ans.nth_back(4096 - 1);
            ans
        };
        assert_eq!(partial_all_harts.clone().count(), usize::MAX - 65536 - 4096);
        assert_eq!(partial_all_harts.clone().last(), Some(usize::MAX - 4096));
        assert_eq!(partial_all_harts.clone().min(), Some(65536));
        assert_eq!(partial_all_harts.clone().max(), Some(usize::MAX - 4096));
        assert!(partial_all_harts.is_sorted());

        let nothing = HartMask::from_mask_base(0, 1000);
        assert_eq!(nothing.iter().count(), 0);
        assert_eq!(nothing.iter().last(), None);
        assert_eq!(nothing.iter().min(), None);
        assert_eq!(nothing.iter().max(), None);
        assert!(nothing.iter().is_sorted());

        let mask = HartMask::from_mask_base(0b101011, 1);
        assert_eq!(mask.iter().count(), 4);
        assert_eq!(mask.iter().last(), Some(6));
        assert_eq!(mask.iter().min(), Some(1));
        assert_eq!(mask.iter().max(), Some(6));
        assert!(mask.iter().is_sorted());

        let all_mask_bits_set = HartMask::from_mask_base(usize::MAX, 1000);
        let last = 1000 + usize::BITS as usize - 1;
        assert_eq!(all_mask_bits_set.iter().count(), usize::BITS as usize);
        assert_eq!(all_mask_bits_set.iter().last(), Some(last));
        assert_eq!(all_mask_bits_set.iter().min(), Some(1000));
        assert_eq!(all_mask_bits_set.iter().max(), Some(last));
        assert!(all_mask_bits_set.iter().is_sorted());
    }

    #[test]
    fn rustsbi_counter_index_mask() {
        let mask = CounterMask::from_mask_base(0b1, 400);
        assert!(!mask.has_bit(0));
        assert!(mask.has_bit(400));
        assert!(!mask.has_bit(401));
        let mask = CounterMask::from_mask_base(0b110, 500);
        assert!(!mask.has_bit(0));
        assert!(!mask.has_bit(500));
        assert!(mask.has_bit(501));
        assert!(mask.has_bit(502));
        assert!(!mask.has_bit(500 + (usize::BITS as usize)));
        let max_bit = 1 << (usize::BITS - 1);
        let mask = CounterMask::from_mask_base(max_bit, 600);
        assert!(mask.has_bit(600 + (usize::BITS as usize) - 1));
        assert!(!mask.has_bit(600 + (usize::BITS as usize)));
        let mask = CounterMask::from_mask_base(0b11, usize::MAX - 1);
        assert!(!mask.has_bit(usize::MAX - 2));
        assert!(mask.has_bit(usize::MAX - 1));
        assert!(mask.has_bit(usize::MAX));
        assert!(!mask.has_bit(0));
        let mask = CounterMask::from_mask_base(0, usize::MAX);
        let null_mask = CounterMask::from_mask_base(0, 0);
        (0..=usize::BITS as usize).for_each(|i| {
            assert!(mask.has_bit(i));
            assert!(!null_mask.has_bit(i));
        });
        assert!(mask.has_bit(usize::MAX));
    }

    #[test]
    fn rustsbi_mask_non_usize() {
        assert_eq!(CounterMask::<i32>::IGNORE_MASK, -1);
        assert_eq!(CounterMask::<i64>::IGNORE_MASK, -1);
        assert_eq!(CounterMask::<i128>::IGNORE_MASK, -1);
        assert_eq!(CounterMask::<u32>::IGNORE_MASK, u32::MAX);
        assert_eq!(CounterMask::<u64>::IGNORE_MASK, u64::MAX);
        assert_eq!(CounterMask::<u128>::IGNORE_MASK, u128::MAX);

        assert_eq!(HartMask::<i32>::IGNORE_MASK, -1);
        assert_eq!(HartMask::<i64>::IGNORE_MASK, -1);
        assert_eq!(HartMask::<i128>::IGNORE_MASK, -1);
        assert_eq!(HartMask::<u32>::IGNORE_MASK, u32::MAX);
        assert_eq!(HartMask::<u64>::IGNORE_MASK, u64::MAX);
        assert_eq!(HartMask::<u128>::IGNORE_MASK, u128::MAX);

        assert_eq!(HartMask::<i32>::all(), HartMask::from_mask_base(0, -1));
    }
}
