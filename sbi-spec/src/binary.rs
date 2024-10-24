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
pub struct SbiRet {
    /// Error number.
    pub error: usize,
    /// Result value.
    pub value: usize,
}

/// SBI success state return value.
pub const RET_SUCCESS: usize = 0;
/// Error for SBI call failed for unknown reasons.
pub const RET_ERR_FAILED: usize = -1isize as _;
/// Error for target operation not supported.
pub const RET_ERR_NOT_SUPPORTED: usize = -2isize as _;
/// Error for invalid parameter.
pub const RET_ERR_INVALID_PARAM: usize = -3isize as _;
/// Error for denied.
pub const RET_ERR_DENIED: usize = -4isize as _;
/// Error for invalid address.
pub const RET_ERR_INVALID_ADDRESS: usize = -5isize as _;
/// Error for resource already available.
pub const RET_ERR_ALREADY_AVAILABLE: usize = -6isize as _;
/// Error for resource already started.
pub const RET_ERR_ALREADY_STARTED: usize = -7isize as _;
/// Error for resource already stopped.
pub const RET_ERR_ALREADY_STOPPED: usize = -8isize as _;
/// Error for shared memory not available.
pub const RET_ERR_NO_SHMEM: usize = -9isize as _;

impl core::fmt::Debug for SbiRet {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.error {
            RET_SUCCESS => self.value.fmt(f),
            RET_ERR_FAILED => write!(f, "<SBI call failed>"),
            RET_ERR_NOT_SUPPORTED => write!(f, "<SBI feature not supported>"),
            RET_ERR_INVALID_PARAM => write!(f, "<SBI invalid parameter>"),
            RET_ERR_DENIED => write!(f, "<SBI denied>"),
            RET_ERR_INVALID_ADDRESS => write!(f, "<SBI invalid address>"),
            RET_ERR_ALREADY_AVAILABLE => write!(f, "<SBI already available>"),
            RET_ERR_ALREADY_STARTED => write!(f, "<SBI already started>"),
            RET_ERR_ALREADY_STOPPED => write!(f, "<SBI already stopped>"),
            RET_ERR_NO_SHMEM => write!(f, "<SBI shared memory not available>"),
            unknown => write!(f, "[SBI Unknown error: {unknown:#x}]"),
        }
    }
}

/// RISC-V SBI error in enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
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
    /// Custom error code.
    Custom(isize),
}

impl SbiRet {
    /// Returns success SBI state with given `value`.
    #[inline]
    pub const fn success(value: usize) -> Self {
        Self {
            error: RET_SUCCESS,
            value,
        }
    }

    /// The SBI call request failed for unknown reasons.
    #[inline]
    pub const fn failed() -> Self {
        Self {
            error: RET_ERR_FAILED,
            value: 0,
        }
    }

    /// SBI call failed due to not supported by target ISA,
    /// operation type not supported,
    /// or target operation type not implemented on purpose.
    #[inline]
    pub const fn not_supported() -> Self {
        Self {
            error: RET_ERR_NOT_SUPPORTED,
            value: 0,
        }
    }

    /// SBI call failed due to invalid hart mask parameter,
    /// invalid target hart id,
    /// invalid operation type,
    /// or invalid resource index.
    #[inline]
    pub const fn invalid_param() -> Self {
        Self {
            error: RET_ERR_INVALID_PARAM,
            value: 0,
        }
    }
    /// SBI call denied for unsatisfied entry criteria, or insufficient access
    /// permission to debug console or CPPC register.
    #[inline]
    pub const fn denied() -> Self {
        Self {
            error: RET_ERR_DENIED,
            value: 0,
        }
    }

    /// SBI call failed for invalid mask start address,
    /// not a valid physical address parameter,
    /// or the target address is prohibited by PMP to run in supervisor mode.
    #[inline]
    pub const fn invalid_address() -> Self {
        Self {
            error: RET_ERR_INVALID_ADDRESS,
            value: 0,
        }
    }

    /// SBI call failed for the target resource is already available,
    /// e.g., the target hart is already started when caller still requests it to start.
    #[inline]
    pub const fn already_available() -> Self {
        Self {
            error: RET_ERR_ALREADY_AVAILABLE,
            value: 0,
        }
    }

    /// SBI call failed for the target resource is already started,
    /// e.g., target performance counter is started.
    #[inline]
    pub const fn already_started() -> Self {
        Self {
            error: RET_ERR_ALREADY_STARTED,
            value: 0,
        }
    }

    /// SBI call failed for the target resource is already stopped,
    /// e.g., target performance counter is stopped.
    #[inline]
    pub const fn already_stopped() -> Self {
        Self {
            error: RET_ERR_ALREADY_STOPPED,
            value: 0,
        }
    }

    /// SBI call failed for shared memory is not available,
    /// e.g. nested acceleration shared memory is not available.
    #[inline]
    pub const fn no_shmem() -> Self {
        Self {
            error: RET_ERR_NO_SHMEM,
            value: 0,
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
    } else if bit < base {
        // invalid index, under minimum range.
        false
    } else if (bit - base) >= usize::BITS as usize {
        // invalid index, over max range.
        false
    } else {
        // index is in range, check if it is set in the mask.
        mask & (1 << (bit - base)) != 0
    }
}

/// Hart mask structure in SBI function calls.
#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct HartMask {
    hart_mask: usize,
    hart_mask_base: usize,
}

impl HartMask {
    /// Special value to ignore the `mask`, and consider all `bit`s as set.
    pub const IGNORE_MASK: usize = usize::MAX;

    /// Construct a [HartMask] from mask value and base hart id.
    #[inline]
    pub const fn from_mask_base(hart_mask: usize, hart_mask_base: usize) -> Self {
        Self {
            hart_mask,
            hart_mask_base,
        }
    }

    /// Gets the special value for ignoring the `mask` parameter.
    #[inline]
    pub const fn ignore_mask(&self) -> usize {
        Self::IGNORE_MASK
    }

    /// Returns `mask` and `base` parameters from the [HartMask].
    #[inline]
    pub const fn into_inner(self) -> (usize, usize) {
        (self.hart_mask, self.hart_mask_base)
    }

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
}

/// Counter index mask structure in SBI function calls for the `PMU` extension §11.
#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct CounterMask {
    counter_idx_mask: usize,
    counter_idx_base: usize,
}

impl CounterMask {
    /// Special value to ignore the `mask`, and consider all `bit`s as set.
    pub const IGNORE_MASK: usize = usize::MAX;

    /// Construct a [CounterMask] from mask value and base counter index.
    #[inline]
    pub const fn from_mask_base(counter_idx_mask: usize, counter_idx_base: usize) -> Self {
        Self {
            counter_idx_mask,
            counter_idx_base,
        }
    }

    /// Gets the special value for ignoring the `mask` parameter.
    #[inline]
    pub const fn ignore_mask(&self) -> usize {
        Self::IGNORE_MASK
    }

    /// Returns `mask` and `base` parameters from the [CounterMask].
    #[inline]
    pub const fn into_inner(self) -> (usize, usize) {
        (self.counter_idx_mask, self.counter_idx_base)
    }

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
}
