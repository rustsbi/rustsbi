//! Reference-free APLIC MSI-delivery configuration.
//!
//! This module accepts an already-owned MMIO capability and never converts a
//! physical address into a Rust reference or pointer.

use crate::peripheral::aplic::{
    DomainConfig, MachineMsiAddrCfgH, SourceConfig, SupervisorMsiAddrCfgH,
};

const MIN_CONTROL_REGION_SIZE: usize = 0x4000;
const MAX_SOURCE_COUNT: u32 = 1023;
const DOMAINCFG: usize = 0x0000;
const SOURCECFG_BASE: usize = 0x0004;
const MMSICFGADDR: usize = 0x1bc0;
const MMSICFGADDRH: usize = 0x1bc4;
const SMSICFGADDR: usize = 0x1bc8;
const SMSICFGADDRH: usize = 0x1bcc;
const CLRIE_BASE: usize = 0x1f00;
const DOMAINCFG_READ_ONLY: u32 = 0x80 << 24;

/// Performs 32-bit accesses within an already-owned APLIC control window.
pub trait VolatileAccess {
    /// Backend-specific MMIO-access failure.
    type Error;

    /// Returns the size of the owned register window in bytes.
    fn len(&self) -> usize;

    /// Returns whether the owned register window has no bytes.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Reads one aligned 32-bit register at an offset in the owned window.
    fn read_u32(&self, offset: usize) -> Result<u32, Self::Error>;

    /// Writes one aligned 32-bit register at an offset in the owned window.
    fn write_u32(&self, offset: usize, value: u32) -> Result<(), Self::Error>;

    /// Orders MMIO accesses against surrounding I/O and memory operations.
    fn fence_iorw(&self);
}

/// Validated routing inputs for APLIC MSI-delivery mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MsiModeConfig {
    source_count: u32,
    machine_imsic_base: u64,
    supervisor_imsic_base: u64,
    hart_index_width: u8,
}

impl MsiModeConfig {
    /// Validates the fixed routing inputs used by this APLIC configuration path.
    pub const fn new(
        source_count: u32,
        machine_imsic_base: u64,
        supervisor_imsic_base: u64,
        hart_index_width: u8,
    ) -> Option<Self> {
        if source_count == 0
            || source_count > MAX_SOURCE_COUNT
            || hart_index_width > 7
            || !machine_imsic_base.is_multiple_of(0x1000)
            || !supervisor_imsic_base.is_multiple_of(0x1000)
            || machine_imsic_base >> 56 != 0
            || supervisor_imsic_base >> 56 != 0
        {
            None
        } else {
            Some(Self {
                source_count,
                machine_imsic_base,
                supervisor_imsic_base,
                hart_index_width,
            })
        }
    }
}

/// The failed APLIC MMIO operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AplicOperation {
    /// A register read.
    Read,
    /// A register write.
    Write,
}

/// Failure while configuring APLIC MSI delivery.
#[derive(Debug, Eq, PartialEq)]
pub enum ConfigureMsiError<E> {
    /// The caller-owned window cannot contain the fixed APLIC control region.
    WindowTooSmall {
        /// Required size of an APLIC control region.
        required: usize,
        /// Size reported by the access capability.
        actual: usize,
    },
    /// The access capability rejected one MMIO operation.
    Backend {
        /// Failed MMIO operation.
        operation: AplicOperation,
        /// Register offset for the failed operation.
        offset: usize,
        /// Backend-specific failure.
        error: E,
    },
    /// `domaincfg` did not establish the required little-endian MSI mode.
    DomainConfigMismatch {
        /// Required observable register value.
        expected: u32,
        /// Observed register value.
        actual: u32,
    },
    /// Machine MSI address configuration was already locked.
    Locked,
    /// A writable register did not retain the requested value.
    ReadbackMismatch {
        /// Register offset that failed readback.
        offset: usize,
        /// Requested value.
        expected: u32,
        /// Observed value.
        actual: u32,
    },
}

/// Configures one machine-level APLIC root domain for MSI forwarding.
///
/// The operation establishes little-endian MSI delivery before examining the
/// lock bit, clears all interrupt-enable words, programs both machine and
/// supervisor IMSIC address layouts, delegates each supported source to child
/// index zero, and fences before success.
pub fn configure_msi_mode<A: VolatileAccess>(
    access: &A,
    config: MsiModeConfig,
) -> Result<(), ConfigureMsiError<A::Error>> {
    if access.len() < MIN_CONTROL_REGION_SIZE {
        return Err(ConfigureMsiError::WindowTooSmall {
            required: MIN_CONTROL_REGION_SIZE,
            actual: access.len(),
        });
    }

    let domain = DomainConfig::from_raw(0)
        .set_delivery_mode(1)
        .set_big_endian(false)
        .set_interrupt_enable(false)
        .raw();
    let expected_domain = DOMAINCFG_READ_ONLY | domain;
    write(access, DOMAINCFG, expected_domain)?;
    let actual_domain = read(access, DOMAINCFG)?;
    if actual_domain != expected_domain {
        return Err(ConfigureMsiError::DomainConfigMismatch {
            expected: expected_domain,
            actual: actual_domain,
        });
    }

    if MachineMsiAddrCfgH::from_raw(read(access, MMSICFGADDRH)?).lock() {
        return Err(ConfigureMsiError::Locked);
    }

    for source in (0..=config.source_count).step_by(32) {
        let offset = CLRIE_BASE + (source as usize / 32) * 4;
        write_unverified(access, offset, u32::MAX)?;
    }

    let (machine_low, machine_high) = machine_msi_address(config);
    let (supervisor_low, supervisor_high) = supervisor_msi_address(config);
    write_and_verify(access, MMSICFGADDR, machine_low)?;
    write_and_verify(access, MMSICFGADDRH, machine_high)?;
    write_and_verify(access, SMSICFGADDR, supervisor_low)?;
    write_and_verify(access, SMSICFGADDRH, supervisor_high)?;

    let source_config = SourceConfig::from_raw(0).set_delegate(true).raw();
    for source in 1..=config.source_count {
        let offset = SOURCECFG_BASE + (source as usize - 1) * 4;
        write_and_verify(access, offset, source_config)?;
    }
    access.fence_iorw();
    Ok(())
}

fn machine_msi_address(config: MsiModeConfig) -> (u32, u32) {
    let page_number = base_page_number(config.machine_imsic_base, config.hart_index_width);
    let high = MachineMsiAddrCfgH::from_raw(0)
        .set_low_hart_index_width(config.hart_index_width)
        .set_high_base_ppn((page_number >> 32) as u16)
        .raw();
    (page_number as u32, high)
}

fn supervisor_msi_address(config: MsiModeConfig) -> (u32, u32) {
    let page_number = base_page_number(config.supervisor_imsic_base, config.hart_index_width);
    let high = SupervisorMsiAddrCfgH::from_raw(0)
        .set_high_base_ppn((page_number >> 32) as u16)
        .raw();
    (page_number as u32, high)
}

fn base_page_number(base: u64, hart_index_width: u8) -> u64 {
    let hart_mask = (1u64 << hart_index_width) - 1;
    (base >> 12) & !hart_mask
}

fn read<A: VolatileAccess>(access: &A, offset: usize) -> Result<u32, ConfigureMsiError<A::Error>> {
    access
        .read_u32(offset)
        .map_err(|error| ConfigureMsiError::Backend {
            operation: AplicOperation::Read,
            offset,
            error,
        })
}

fn write<A: VolatileAccess>(
    access: &A,
    offset: usize,
    value: u32,
) -> Result<(), ConfigureMsiError<A::Error>> {
    write_unverified(access, offset, value)
}

fn write_unverified<A: VolatileAccess>(
    access: &A,
    offset: usize,
    value: u32,
) -> Result<(), ConfigureMsiError<A::Error>> {
    access
        .write_u32(offset, value)
        .map_err(|error| ConfigureMsiError::Backend {
            operation: AplicOperation::Write,
            offset,
            error,
        })
}

fn write_and_verify<A: VolatileAccess>(
    access: &A,
    offset: usize,
    value: u32,
) -> Result<(), ConfigureMsiError<A::Error>> {
    write(access, offset, value)?;
    let actual = read(access, offset)?;
    if actual == value {
        Ok(())
    } else {
        Err(ConfigureMsiError::ReadbackMismatch {
            offset,
            expected: value,
            actual,
        })
    }
}

#[cfg(test)]
mod tests {
    use core::cell::{Cell, RefCell};

    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TestError {
        Access,
    }

    struct Backend {
        registers: RefCell<[u32; MIN_CONTROL_REGION_SIZE / 4]>,
        writes: RefCell<std::vec::Vec<(usize, u32)>>,
        fence_count: Cell<usize>,
        fail_write: Cell<bool>,
        readback_override: Cell<Option<(usize, u32)>>,
    }

    impl Backend {
        fn new() -> Self {
            Self {
                registers: RefCell::new([0; MIN_CONTROL_REGION_SIZE / 4]),
                writes: RefCell::new(std::vec::Vec::new()),
                fence_count: Cell::new(0),
                fail_write: Cell::new(false),
                readback_override: Cell::new(None),
            }
        }
    }

    impl VolatileAccess for Backend {
        type Error = TestError;

        fn len(&self) -> usize {
            MIN_CONTROL_REGION_SIZE
        }

        fn read_u32(&self, offset: usize) -> Result<u32, Self::Error> {
            if let Some((target, value)) = self.readback_override.get()
                && target == offset
            {
                return Ok(value);
            }
            Ok(self.registers.borrow()[offset / 4])
        }

        fn write_u32(&self, offset: usize, value: u32) -> Result<(), Self::Error> {
            if self.fail_write.get() {
                return Err(TestError::Access);
            }
            self.writes.borrow_mut().push((offset, value));
            self.registers.borrow_mut()[offset / 4] = value;
            Ok(())
        }

        fn fence_iorw(&self) {
            self.fence_count.set(self.fence_count.get() + 1);
        }
    }

    fn config() -> MsiModeConfig {
        MsiModeConfig::new(2, 0x2400_0000, 0x2800_0000, 2).unwrap()
    }

    #[test]
    fn configuration_establishes_little_endian_msi_routing() {
        let backend = Backend::new();
        configure_msi_mode(&backend, config()).unwrap();

        let registers = backend.registers.borrow();
        assert_eq!(registers[DOMAINCFG / 4], 0x8000_0004);
        assert_eq!(registers[MMSICFGADDRH / 4], 2 << 12);
        assert_eq!(registers[SMSICFGADDRH / 4], 0);
        assert_eq!(registers[SOURCECFG_BASE / 4], 1 << 10);
        assert_eq!(backend.fence_count.get(), 1);
        assert_eq!(
            backend.writes.borrow().as_slice(),
            &[
                (DOMAINCFG, 0x8000_0004),
                (CLRIE_BASE, u32::MAX),
                (MMSICFGADDR, 0x0002_4000),
                (MMSICFGADDRH, 2 << 12),
                (SMSICFGADDR, 0x0002_8000),
                (SMSICFGADDRH, 0),
                (SOURCECFG_BASE, 1 << 10),
                (SOURCECFG_BASE + 4, 1 << 10),
            ]
        );
    }

    #[test]
    fn hart_index_bits_are_not_encoded_as_a_base_address() {
        let config = MsiModeConfig::new(1, 0x2400_3000, 0x2800_3000, 2).unwrap();
        assert_eq!(machine_msi_address(config).0, 0x2400_0000 >> 12);
        assert_eq!(supervisor_msi_address(config).0, 0x2800_0000 >> 12);
    }

    #[test]
    fn locked_configuration_stops_before_routing_writes() {
        let backend = Backend::new();
        backend.registers.borrow_mut()[MMSICFGADDRH / 4] = 1 << 31;
        assert_eq!(
            configure_msi_mode(&backend, config()),
            Err(ConfigureMsiError::Locked)
        );
        assert_eq!(
            backend.writes.borrow().as_slice(),
            &[(DOMAINCFG, 0x8000_0004)]
        );
    }

    #[test]
    fn backend_failure_is_reported() {
        let backend = Backend::new();
        backend.fail_write.set(true);
        assert!(matches!(
            configure_msi_mode(&backend, config()),
            Err(ConfigureMsiError::Backend {
                operation: AplicOperation::Write,
                ..
            })
        ));
    }

    #[test]
    fn domain_mode_must_read_back_with_its_endian_marker() {
        let backend = Backend::new();
        backend.readback_override.set(Some((DOMAINCFG, 0)));
        assert!(matches!(
            configure_msi_mode(&backend, config()),
            Err(ConfigureMsiError::DomainConfigMismatch { .. })
        ));
    }

    #[test]
    fn every_writable_address_configuration_is_verified() {
        let backend = Backend::new();
        backend.readback_override.set(Some((MMSICFGADDR, u32::MAX)));
        assert!(matches!(
            configure_msi_mode(&backend, config()),
            Err(ConfigureMsiError::ReadbackMismatch {
                offset: MMSICFGADDR,
                ..
            })
        ));
    }

    #[test]
    fn configuration_inputs_are_bounded_before_access() {
        assert!(MsiModeConfig::new(0, 0x2400_0000, 0x2800_0000, 2).is_none());
        assert!(MsiModeConfig::new(1024, 0x2400_0000, 0x2800_0000, 2).is_none());
        assert!(MsiModeConfig::new(1, 0x2400_0001, 0x2800_0000, 2).is_none());
        assert!(MsiModeConfig::new(1, 1 << 56, 0x2800_0000, 2).is_none());
    }

    struct ShortBackend;

    impl VolatileAccess for ShortBackend {
        type Error = TestError;

        fn len(&self) -> usize {
            MIN_CONTROL_REGION_SIZE - 4
        }

        fn read_u32(&self, _: usize) -> Result<u32, Self::Error> {
            unreachable!()
        }

        fn write_u32(&self, _: usize, _: u32) -> Result<(), Self::Error> {
            unreachable!()
        }

        fn fence_iorw(&self) {
            unreachable!()
        }
    }

    #[test]
    fn short_windows_are_rejected_before_access() {
        assert!(matches!(
            configure_msi_mode(&ShortBackend, config()),
            Err(ConfigureMsiError::WindowTooSmall { .. })
        ));
    }
}
