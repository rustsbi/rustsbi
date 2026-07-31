//! Reference-free IMSIC configuration operations.
//!
//! The abstractions in this module operate on a caller-owned CSR capability.
//! They never form pointers or references to IMSIC memory-mapped registers.

use crate::Iid;
use crate::peripheral::imsic::select;

/// The highest implemented IMSIC identity in one interrupt file.
///
/// Ratified AIA permits only values one less than a multiple of 64, from 63
/// through 2047 inclusive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct IdentityCount(u16);

impl IdentityCount {
    /// Attempts to construct a valid highest implemented identity.
    #[inline]
    pub const fn new(max_identity: u16) -> Option<Self> {
        if max_identity < 63 || max_identity > 2047 || max_identity % 64 != 63 {
            None
        } else {
            Some(Self(max_identity))
        }
    }

    /// Returns the highest implemented identity.
    #[inline]
    pub const fn max_identity(self) -> u16 {
        self.0
    }

    /// Returns the number of XLEN-wide `eip` or `eie` registers to clear.
    #[inline]
    pub const fn register_count(self, xlen: Xlen) -> usize {
        self.0 as usize / xlen.word_bits() + 1
    }
}

/// The XLEN used for an IMSIC indirect-register access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Xlen {
    /// RV32 access width.
    X32,
    /// RV64 access width.
    X64,
}

impl Xlen {
    /// Returns the number of bits in an indirectly accessed register.
    #[inline]
    pub const fn word_bits(self) -> usize {
        match self {
            Self::X32 => 32,
            Self::X64 => 64,
        }
    }

    /// Returns the selector distance between adjacent implemented registers.
    #[inline]
    pub const fn selector_stride(self) -> usize {
        match self {
            Self::X32 => 1,
            Self::X64 => 2,
        }
    }
}

/// One selected bit in an IMSIC indirect register.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndirectLocation {
    selector: usize,
    bit: usize,
}

impl IndirectLocation {
    /// Returns the `*iselect` value for this register.
    #[inline]
    pub const fn selector(self) -> usize {
        self.selector
    }

    /// Returns the bit position in the selected XLEN-wide register.
    #[inline]
    pub const fn bit(self) -> usize {
        self.bit
    }

    /// Returns a word with only this identity enabled or pending.
    #[inline]
    pub const fn bit_mask(self) -> usize {
        1usize << self.bit
    }
}

/// Returns the `eip` location for an interrupt identity.
#[inline]
pub const fn eip_location(identity: Iid, xlen: Xlen) -> IndirectLocation {
    location(identity, xlen, select::EIP_BASE as usize)
}

/// Returns the `eie` location for an interrupt identity.
#[inline]
pub const fn eie_location(identity: Iid, xlen: Xlen) -> IndirectLocation {
    location(identity, xlen, select::EIE_BASE as usize)
}

const fn location(identity: Iid, xlen: Xlen, base: usize) -> IndirectLocation {
    let identity = identity.number() as usize;
    let word_bits = xlen.word_bits();
    IndirectLocation {
        selector: base + (identity / word_bits) * xlen.selector_stride(),
        bit: identity % word_bits,
    }
}

/// A validated IMSIC interrupt-file initialization request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileConfig {
    identity_count: IdentityCount,
    notification_identity: Iid,
}

impl FileConfig {
    /// Combines the implemented identity limit with a firmware notification IID.
    #[inline]
    pub const fn new(identity_count: IdentityCount, notification_identity: Iid) -> Option<Self> {
        if notification_identity.number() > identity_count.max_identity() {
            None
        } else {
            Some(Self {
                identity_count,
                notification_identity,
            })
        }
    }

    /// Returns the implemented identity limit.
    #[inline]
    pub const fn identity_count(self) -> IdentityCount {
        self.identity_count
    }

    /// Returns the identity reserved for firmware notifications.
    #[inline]
    pub const fn notification_identity(self) -> Iid {
        self.notification_identity
    }
}

/// Accesses the machine-level IMSIC indirect CSR window.
///
/// Implementations must preserve their own fault boundary. This trait is
/// deliberately free of CSR numbers and inline assembly.
pub trait MachineIndirectCsr {
    /// Backend-specific CSR-access failure.
    type Error;

    /// Swaps the indirect-register selector and returns its old value.
    fn swap_select(&self, value: usize) -> Result<usize, Self::Error>;

    /// Reads the currently selected indirect register.
    fn read_indirect(&self) -> Result<usize, Self::Error>;

    /// Swaps the currently selected indirect register and returns its old value.
    fn swap_indirect(&self, value: usize) -> Result<usize, Self::Error>;
}

/// The stage of one indirect-register transaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndirectOperation {
    /// Selecting an indirect register.
    Select,
    /// Writing an indirect register.
    Write,
    /// Reading an indirect register for verification.
    Read,
    /// Restoring the selector after one transaction.
    Restore,
}

/// One indirect-register access or verification failure.
#[derive(Debug, Eq, PartialEq)]
pub enum IndirectAccessError<E> {
    /// The caller-owned CSR capability reported an access failure.
    Backend(E),
    /// A readback differed from the value required by the operation.
    Readback {
        /// Value written by the operation.
        expected: usize,
        /// Value observed through the same selected register.
        actual: usize,
    },
}

/// A failure tagged with its transaction stage.
#[derive(Debug, Eq, PartialEq)]
pub struct IndirectFailure<E> {
    /// Transaction stage that failed.
    pub operation: IndirectOperation,
    /// Backend or readback failure observed at this stage.
    pub error: IndirectAccessError<E>,
}

/// Failure while initializing an IMSIC interrupt file.
#[derive(Debug, Eq, PartialEq)]
pub struct InitializeError<E> {
    /// The first failed transaction stage.
    pub primary: IndirectFailure<E>,
    /// A failed selector restoration attempted after `primary`, if any.
    pub restoration: Option<IndirectFailure<E>>,
}

/// Initializes one machine IMSIC interrupt file through a fallible CSR capability.
///
/// The operation disables delivery, clears all implemented `eip` and `eie`
/// words, enables only the configured notification identity, and re-enables
/// delivery. Every selected-register operation restores the prior selector.
pub fn initialize_machine_file<C: MachineIndirectCsr>(
    csr: &C,
    xlen: Xlen,
    config: FileConfig,
) -> Result<(), InitializeError<C::Error>> {
    write_verified(csr, select::EIDELIVERY as usize, 0)?;
    write_verified(csr, select::EITHRESHOLD as usize, 0)?;

    let register_count = config.identity_count().register_count(xlen);
    let stride = xlen.selector_stride();
    for word in 0..register_count {
        write_verified(csr, select::EIP_BASE as usize + word * stride, 0)?;
        write_verified(csr, select::EIE_BASE as usize + word * stride, 0)?;
    }

    let notification = eie_location(config.notification_identity(), xlen);
    write_verified(csr, notification.selector(), notification.bit_mask())?;
    write_verified(csr, select::EIDELIVERY as usize, 1)
}

fn write_verified<C: MachineIndirectCsr>(
    csr: &C,
    selector: usize,
    value: usize,
) -> Result<(), InitializeError<C::Error>> {
    let original = csr.swap_select(selector).map_err(|error| InitializeError {
        primary: backend_failure(IndirectOperation::Select, error),
        restoration: None,
    })?;

    let primary = match csr.swap_indirect(value) {
        Ok(_) => match csr.read_indirect() {
            Ok(actual) if actual == value => return restore_after_success(csr, original, selector),
            Ok(actual) => IndirectFailure {
                operation: IndirectOperation::Read,
                error: IndirectAccessError::Readback {
                    expected: value,
                    actual,
                },
            },
            Err(error) => backend_failure(IndirectOperation::Read, error),
        },
        Err(error) => backend_failure(IndirectOperation::Write, error),
    };
    Err(InitializeError {
        primary,
        restoration: restore_selector(csr, original, selector).err(),
    })
}

fn restore_after_success<C: MachineIndirectCsr>(
    csr: &C,
    original: usize,
    selected: usize,
) -> Result<(), InitializeError<C::Error>> {
    restore_selector(csr, original, selected).map_err(|primary| InitializeError {
        primary,
        restoration: None,
    })
}

fn restore_selector<C: MachineIndirectCsr>(
    csr: &C,
    original: usize,
    selected: usize,
) -> Result<(), IndirectFailure<C::Error>> {
    match csr.swap_select(original) {
        Ok(actual) if actual == selected => Ok(()),
        Ok(actual) => Err(IndirectFailure {
            operation: IndirectOperation::Restore,
            error: IndirectAccessError::Readback {
                expected: selected,
                actual,
            },
        }),
        Err(error) => Err(backend_failure(IndirectOperation::Restore, error)),
    }
}

fn backend_failure<E>(operation: IndirectOperation, error: E) -> IndirectFailure<E> {
    IndirectFailure {
        operation,
        error: IndirectAccessError::Backend(error),
    }
}

#[cfg(test)]
mod tests {
    use core::cell::{Cell, RefCell};

    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TestError {
        Select,
        Read,
        Write,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Call {
        Select(usize),
        Read,
        Write(usize),
    }

    struct Backend {
        selector: Cell<usize>,
        registers: RefCell<[usize; 256]>,
        calls: RefCell<std::vec::Vec<Call>>,
        select_calls: Cell<usize>,
        fail_select_on: Cell<Option<usize>>,
        fail_read: Cell<bool>,
        fail_write: Cell<bool>,
        read_override: Cell<Option<usize>>,
    }

    impl Backend {
        fn new(selector: usize) -> Self {
            Self {
                selector: Cell::new(selector),
                registers: RefCell::new([0; 256]),
                calls: RefCell::new(std::vec::Vec::new()),
                select_calls: Cell::new(0),
                fail_select_on: Cell::new(None),
                fail_read: Cell::new(false),
                fail_write: Cell::new(false),
                read_override: Cell::new(None),
            }
        }
    }

    impl MachineIndirectCsr for Backend {
        type Error = TestError;

        fn swap_select(&self, value: usize) -> Result<usize, Self::Error> {
            self.calls.borrow_mut().push(Call::Select(value));
            let call = self.select_calls.get();
            self.select_calls.set(call + 1);
            if self.fail_select_on.get() == Some(call) {
                return Err(TestError::Select);
            }
            Ok(self.selector.replace(value))
        }

        fn read_indirect(&self) -> Result<usize, Self::Error> {
            self.calls.borrow_mut().push(Call::Read);
            if self.fail_read.get() {
                return Err(TestError::Read);
            }
            Ok(self
                .read_override
                .get()
                .unwrap_or_else(|| self.registers.borrow()[self.selector.get()]))
        }

        fn swap_indirect(&self, value: usize) -> Result<usize, Self::Error> {
            self.calls.borrow_mut().push(Call::Write(value));
            if self.fail_write.get() {
                return Err(TestError::Write);
            }
            let selector = self.selector.get();
            let mut registers = self.registers.borrow_mut();
            let old = registers[selector];
            registers[selector] = value;
            Ok(old)
        }
    }

    fn config(max_identity: u16, notification: u16) -> FileConfig {
        FileConfig::new(
            IdentityCount::new(max_identity).unwrap(),
            Iid::new(notification).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn identity_count_accepts_only_ratified_limits() {
        for identity in [63, 127, 2047] {
            assert!(IdentityCount::new(identity).is_some());
        }
        for identity in [0, 62, 64, 128, 2048] {
            assert!(IdentityCount::new(identity).is_none());
        }
    }

    #[test]
    fn locations_follow_xlen_and_selector_stride() {
        let iid = Iid::new(64).unwrap();
        assert_eq!(eip_location(iid, Xlen::X32).selector(), 0x82);
        assert_eq!(eip_location(iid, Xlen::X32).bit(), 0);
        assert_eq!(eip_location(iid, Xlen::X64).selector(), 0x82);
        assert_eq!(eip_location(iid, Xlen::X64).bit(), 0);

        let iid = Iid::new(2047).unwrap();
        assert_eq!(eie_location(iid, Xlen::X32).selector(), 0xff);
        assert_eq!(eie_location(iid, Xlen::X32).bit(), 31);
        assert_eq!(eie_location(iid, Xlen::X64).selector(), 0xfe);
        assert_eq!(eie_location(iid, Xlen::X64).bit(), 63);
    }

    #[test]
    fn initialization_restores_the_selector_and_uses_even_rv64_selectors() {
        let backend = Backend::new(0x51);
        initialize_machine_file(&backend, Xlen::X64, config(63, 1)).unwrap();

        assert_eq!(backend.selector.get(), 0x51);
        assert_eq!(backend.registers.borrow()[0x70], 1);
        assert_eq!(backend.registers.borrow()[0xc0], 1 << 1);
        for call in backend.calls.borrow().iter().copied() {
            if let Call::Select(selector) = call
                && (0x80..=0xff).contains(&selector)
            {
                assert_eq!(selector % 2, 0);
            }
        }
    }

    #[test]
    fn initialization_restores_selector_after_a_read_failure() {
        let backend = Backend::new(0x51);
        backend.fail_read.set(true);
        let error = initialize_machine_file(&backend, Xlen::X32, config(63, 1)).unwrap_err();

        assert_eq!(error.primary.operation, IndirectOperation::Read);
        assert!(error.restoration.is_none());
        assert_eq!(backend.selector.get(), 0x51);
    }

    #[test]
    fn initialization_restores_selector_after_a_write_failure() {
        let backend = Backend::new(0x51);
        backend.fail_write.set(true);
        let error = initialize_machine_file(&backend, Xlen::X32, config(63, 1)).unwrap_err();

        assert_eq!(error.primary.operation, IndirectOperation::Write);
        assert!(error.restoration.is_none());
        assert_eq!(backend.selector.get(), 0x51);
    }

    #[test]
    fn initialization_reports_a_selection_failure_without_restoration() {
        let backend = Backend::new(0x51);
        backend.fail_select_on.set(Some(0));
        let error = initialize_machine_file(&backend, Xlen::X32, config(63, 1)).unwrap_err();

        assert_eq!(error.primary.operation, IndirectOperation::Select);
        assert!(error.restoration.is_none());
        assert_eq!(backend.selector.get(), 0x51);
    }

    #[test]
    fn initialization_restores_selector_after_a_readback_mismatch() {
        let backend = Backend::new(0x51);
        backend.read_override.set(Some(usize::MAX));
        let error = initialize_machine_file(&backend, Xlen::X32, config(63, 1)).unwrap_err();

        assert_eq!(error.primary.operation, IndirectOperation::Read);
        assert!(matches!(
            error.primary.error,
            IndirectAccessError::Readback { .. }
        ));
        assert!(error.restoration.is_none());
        assert_eq!(backend.selector.get(), 0x51);
    }

    #[test]
    fn initialization_reports_a_failed_restoration() {
        let backend = Backend::new(0x51);
        backend.fail_select_on.set(Some(1));
        let error = initialize_machine_file(&backend, Xlen::X32, config(63, 1)).unwrap_err();

        assert_eq!(error.primary.operation, IndirectOperation::Restore);
        assert!(error.restoration.is_none());
        assert_eq!(backend.selector.get(), 0x70);
    }

    #[test]
    fn initialization_retains_the_primary_error_when_restoration_also_fails() {
        let backend = Backend::new(0x51);
        backend.fail_write.set(true);
        backend.fail_select_on.set(Some(1));
        let error = initialize_machine_file(&backend, Xlen::X32, config(63, 1)).unwrap_err();

        assert_eq!(error.primary.operation, IndirectOperation::Write);
        assert!(matches!(
            error.restoration,
            Some(IndirectFailure {
                operation: IndirectOperation::Restore,
                ..
            })
        ));
        assert_eq!(backend.selector.get(), 0x70);
    }
}
