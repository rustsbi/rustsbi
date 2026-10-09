//! Current-hart hardware performance counter operations.
//!
//! SBI event mapping and firmware counters belong to policy firmware.
//! Runtime owns discovery, hardware counter selection, and CSR access ordering.

#![forbid(unsafe_code)]

use alloc::boxed::Box;
use core::marker::PhantomData;
use spin::Once;

mod csr;
use crate::csr::{Mcountinhibit, Readable, SUPERVISOR_COUNTER_BASE, Writable};
use crate::hart::HartId;

/// Detected hardware counter positions, widths, and inhibition support for one hart.
/// This copyable description does not authorize access to any counter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CounterTopology {
    mask: u32,
    widths: [u8; 32],
    inhibit_supported: bool,
}

impl CounterTopology {
    /// Returns counter positions used by architectural and device-tree event masks.
    /// Bit 1 describes time, whose control belongs to [`crate::timer::Timer`].
    pub const fn mask(self) -> u32 {
        self.mask
    }
}

static TOPOLOGIES: Once<Box<[Once<CounterTopology>]>> = Once::new();

/// Access to the hardware performance monitor on the current hart.
///
/// The capability cannot be sent or shared across harts. Each operation checks
/// that it is still being used on the hart that created it.
///
/// # Panics
///
/// Initial topology discovery through [`Self::probe`], [`Self::reset`], or
/// [`Self::counter`] panics if probing or restoring hardware counter state fails.
pub struct Pmu {
    hart: HartId,
    _local: PhantomData<*mut ()>,
}

impl Pmu {
    /// Acquires access to the current hart's performance monitor.
    pub fn current() -> Result<Self, CounterError> {
        Ok(Self {
            hart: HartId::current().map_err(|_| CounterError::InvalidHartId)?,
            _local: PhantomData,
        })
    }

    /// Probes and caches this hart's hardware counter topology.
    ///
    /// Programmable counters are stopped while probing. Their values, event
    /// selectors, and inhibition are restored before the topology is published.
    pub fn probe(&self) -> Result<CounterTopology, CounterError> {
        riscv::interrupt::machine::free(|| {
            self.validate()?;
            Ok(probe_topology(self.hart))
        })
    }

    /// Prepares the counters for a fresh supervisor context.
    ///
    /// With `mcountinhibit`, fixed cycle/instruction counters run and
    /// programmable counters stop. Without it, running state is unchanged.
    pub fn reset(&self) -> Result<(), CounterError> {
        riscv::interrupt::machine::free(|| {
            self.validate()?;
            let topology = probe_topology(self.hart);
            if topology.inhibit_supported {
                Mcountinhibit::write((topology.mask & !0b111) as usize)?;
            }
            Ok(())
        })
    }

    /// Resolves an architectural counter position implemented on this hart.
    ///
    /// Positions 0 and 2 are cycle and retired-instruction counters. Position 1
    /// belongs to the timer and has no performance-counter start/stop control.
    pub fn counter(&self, position: usize) -> Result<HardwareCounter, CounterError> {
        let topology = self.probe()?;
        Ok(HardwareCounter {
            hart: self.hart,
            index: counter_index(topology.mask, position)?,
            _local: PhantomData,
        })
    }

    fn validate(&self) -> Result<(), CounterError> {
        let hart = HartId::current().map_err(|_| CounterError::InvalidHartId)?;
        if self.hart != hart {
            return Err(CounterError::WrongHart);
        }
        Ok(())
    }
}

/// Probes and caches the current hart's hardware counter topology.
///
/// Callers mask machine interrupts for the complete discovery operation.
/// Programmable counters are stopped while probing; successful discovery
/// restores their state before publishing the topology.
fn probe_topology(hart: HartId) -> CounterTopology {
    let topologies = TOPOLOGIES.call_once(|| HartId::all().map(|_| Once::new()).collect());
    *topologies[hart.index()].call_once(|| {
        let old_inhibit = match Mcountinhibit::read().map(|bits| bits as u32) {
            Ok(bits) => {
                Mcountinhibit::write((bits | !0b111) as usize)
                    .expect("failed to inhibit performance counters during probing");
                Some(bits)
            }
            Err(crate::trap::Error::UnsupportedInstruction) => None,
            Err(error) => panic!("failed to probe counter inhibition: {error}"),
        };
        let inhibited = if old_inhibit.is_some() {
            Mcountinhibit::read().expect("failed to read back performance counter inhibition")
                as u32
        } else {
            0
        };
        let (programmable_mask, mut widths) = csr::probe_programmable_counters(inhibited);
        widths[0] = 64;
        widths[2] = 64;
        let mask = 0b111 | programmable_mask;
        if let Some(bits) = old_inhibit {
            Mcountinhibit::write(bits as usize)
                .expect("failed to restore counter inhibition after probing");
        }
        CounterTopology {
            mask,
            widths,
            inhibit_supported: old_inhibit.is_some(),
        }
    })
}

/// The architectural function of a hardware counter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CounterKind {
    /// Fixed cycle counter.
    Cycle,
    /// Fixed retired-instruction counter.
    Instructions,
    /// Counter selected by a programmable hardware event.
    Programmable,
}

/// Whether configuring a counter preserves or clears its value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CounterValueUpdate {
    /// Preserve the counter's current value.
    Preserve,
    /// Clear the counter's full value before any requested start.
    Clear,
}

/// Whether configuring a counter also starts it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CounterStartMode {
    /// Preserve the counter's current running state.
    Preserve,
    /// Start the counter after updating its value.
    Start,
}

/// Whether stopping a counter preserves or clears its event selector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CounterEventUpdate {
    /// Preserve the selected event.
    Preserve,
    /// Clear the event selector of a programmable counter.
    Clear,
}

/// Failure to operate on the current hart's hardware counters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CounterError {
    /// The current hart is outside the published boot topology.
    InvalidHartId,
    /// The counter position is outside the implemented topology.
    InvalidCounter,
    /// Time is an architectural indexing hole and has no PMU start/stop control.
    TimeCounter,
    /// A counter handle is being used on a different hart.
    WrongHart,
    /// The counter is already running.
    AlreadyStarted,
    /// The counter is already inhibited.
    AlreadyStopped,
    /// An architectural counter access failed.
    Access(crate::trap::Error),
    /// Configuration failed and restoring its previous state also failed.
    /// The counter state is indeterminate after this error.
    RestoreFailed {
        /// The original configuration failure.
        operation: crate::trap::Error,
        /// The first failure while restoring the previous state.
        restoration: crate::trap::Error,
    },
}

impl From<crate::trap::Error> for CounterError {
    fn from(error: crate::trap::Error) -> Self {
        Self::Access(error)
    }
}

/// A validated hardware counter belonging to the current hart.
/// Its CSR number is only exposed as supervisor discovery data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HardwareCounter {
    hart: HartId,
    index: u8,
    _local: PhantomData<*mut ()>,
}

impl HardwareCounter {
    /// Returns the counter's architectural role.
    pub const fn kind(self) -> CounterKind {
        match self.index {
            0 => CounterKind::Cycle,
            2 => CounterKind::Instructions,
            _ => CounterKind::Programmable,
        }
    }

    /// Returns this counter's implemented precision in bits.
    /// Discovery probes programmable counters while stopped and restores their
    /// full values and event selectors before publishing the cached width.
    pub fn width(self) -> Result<u32, CounterError> {
        riscv::interrupt::machine::free(|| {
            let topology = self.validate()?;
            Ok(u32::from(topology.widths[usize::from(self.index)]))
        })
    }

    /// Tests a device-tree event mask against this counter's position.
    pub const fn matches_mask(self, mask: u32) -> bool {
        mask & (1u32 << self.index) != 0
    }

    /// Returns the supervisor-readable CSR identifier required by SBI discovery.
    pub fn supervisor_csr_number(self) -> u16 {
        SUPERVISOR_COUNTER_BASE + u16::from(self.index)
    }

    /// Reads the full counter value, including both RV32 halves.
    pub fn read(self) -> Result<u64, CounterError> {
        riscv::interrupt::machine::free(|| {
            self.validate()?;
            Ok(csr::read_counter(self.index)?)
        })
    }

    /// Reports the hardware running state when counter inhibition is supported.
    /// Legacy harts without `mcountinhibit` return `None`.
    pub fn is_running(self) -> Result<Option<bool>, CounterError> {
        riscv::interrupt::machine::free(|| {
            let topology = self.validate()?;
            if topology.inhibit_supported {
                Ok(Some(
                    Mcountinhibit::read()? as u32 & self.inhibit_mask() == 0,
                ))
            } else {
                Ok(None)
            }
        })
    }

    /// Configures a counter's event, value, and optional automatic start.
    /// `event` selects a platform-mapped programmable event; `None` preserves
    /// the selector, including the fixed cycle/instruction counter assignments.
    ///
    /// With inhibition, the counter must already be stopped. Runtime snapshots
    /// the state being changed and restores it if any configuration step fails.
    /// A restoration failure reports both errors and leaves the state uncertain.
    ///
    /// Legacy harts cannot report their running state. Programmable counters are
    /// stopped through their selector while clearing their value. Fixed counters
    /// keep advancing; RV32 value writes and restoration are not atomic with
    /// hardware increments.
    pub fn configure(
        self,
        event: Option<u64>,
        value_update: CounterValueUpdate,
        start_mode: CounterStartMode,
    ) -> Result<(), CounterError> {
        riscv::interrupt::machine::free(|| {
            let topology = self.validate()?;
            if event.is_some() && self.kind() != CounterKind::Programmable {
                return Err(CounterError::InvalidCounter);
            }
            configure_counter(
                self.index,
                topology
                    .inhibit_supported
                    .then(|| Mcountinhibit::read().map(|bits| bits as u32))
                    .transpose()?,
                event,
                value_update,
                start_mode,
                |operation| match operation {
                    CounterAccess::WriteInhibit(bits) => {
                        Mcountinhibit::write(bits as usize).map(|()| 0)
                    }
                    CounterAccess::ReadEvent => csr::read_event(self.index),
                    CounterAccess::WriteEvent(value) => {
                        csr::write_event(self.index, value).map(|()| 0)
                    }
                    CounterAccess::ReadValue => csr::read_counter(self.index),
                    CounterAccess::WriteValue(value, mode) => {
                        csr::write_counter(self.index, value, mode).map(|()| 0)
                    }
                },
            )
        })
    }

    /// Starts the counter, optionally replacing its full value before enabling it.
    /// On legacy harts the counter continuously runs, so only the value changes.
    /// RV32 writes on those harts are not atomic with hardware increments.
    pub fn start(self, initial_value: Option<u64>) -> Result<(), CounterError> {
        riscv::interrupt::machine::free(|| {
            let topology = self.validate()?;
            let inhibit = if topology.inhibit_supported {
                let bits = Mcountinhibit::read()? as u32;
                if bits & self.inhibit_mask() == 0 {
                    return Err(CounterError::AlreadyStarted);
                }
                Some(bits)
            } else {
                None
            };
            if let Some(value) = initial_value {
                let write_mode = if inhibit.is_some() {
                    csr::CounterWriteMode::Stopped
                } else {
                    csr::CounterWriteMode::MayRun
                };
                csr::write_counter(self.index, value, write_mode)?;
            }
            if let Some(bits) = inhibit {
                Mcountinhibit::write((bits & !self.inhibit_mask()) as usize)?;
                self.sync_shadow()?;
            }
            Ok(())
        })
    }

    /// Stops the counter and optionally clears its programmable event selector.
    /// Resetting an already stopped counter still clears its selector before
    /// returning `AlreadyStopped`, so policy can release an unused mapping.
    pub fn stop(self, event_update: CounterEventUpdate) -> Result<(), CounterError> {
        riscv::interrupt::machine::free(|| {
            let topology = self.validate()?;
            let inhibit = if topology.inhibit_supported {
                Some(Mcountinhibit::read()? as u32)
            } else {
                None
            };
            if event_update == CounterEventUpdate::Clear && self.kind() == CounterKind::Programmable
            {
                csr::write_event(self.index, 0)?;
            }
            if let Some(bits) = inhibit {
                if bits & self.inhibit_mask() != 0 {
                    return Err(CounterError::AlreadyStopped);
                }
                Mcountinhibit::write((bits | self.inhibit_mask()) as usize)?;
                self.sync_shadow()?;
            }
            Ok(())
        })
    }

    fn validate(self) -> Result<CounterTopology, CounterError> {
        // Each caller already masks machine interrupts for its full operation.
        Pmu {
            hart: self.hart,
            _local: PhantomData,
        }
        .validate()?;
        Ok(probe_topology(self.hart))
    }

    const fn inhibit_mask(self) -> u32 {
        1u32 << self.index
    }

    fn sync_shadow(self) -> Result<(), CounterError> {
        // QEMU observes fixed-counter inhibit changes consistently in their
        // supervisor shadows after a machine-counter read.
        if self.kind() != CounterKind::Programmable {
            csr::read_counter(self.index)?;
        }
        Ok(())
    }
}

// This private access seam keeps the complete per-counter transaction in one
// place and lets failure tests exercise it without executing machine-mode CSRs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CounterAccess {
    WriteInhibit(u32),
    ReadEvent,
    WriteEvent(u64),
    ReadValue,
    WriteValue(u64, csr::CounterWriteMode),
}

fn configure_counter(
    index: u8,
    inhibit: Option<u32>,
    event: Option<u64>,
    value_update: CounterValueUpdate,
    start_mode: CounterStartMode,
    mut access_fn: impl FnMut(CounterAccess) -> Result<u64, crate::trap::Error>,
) -> Result<(), CounterError> {
    let inhibit_mask = 1u32 << index;
    if inhibit.is_some_and(|bits| bits & inhibit_mask == 0) {
        return Err(CounterError::AlreadyStarted);
    }
    let clear_value = value_update == CounterValueUpdate::Clear;
    let temporarily_stop_event = inhibit.is_none() && index >= 3 && clear_value;
    let saved_event = if event.is_some() || temporarily_stop_event {
        Some(access_fn(CounterAccess::ReadEvent)?)
    } else {
        None
    };
    let write_mode = if inhibit.is_some() || temporarily_stop_event {
        csr::CounterWriteMode::Stopped
    } else {
        csr::CounterWriteMode::MayRun
    };
    let mut saved_value = None;
    let result = (|| {
        if temporarily_stop_event {
            access_fn(CounterAccess::WriteEvent(0))?;
            if access_fn(CounterAccess::ReadEvent)? != 0 {
                return Err(crate::trap::Error::UnsupportedInstruction);
            }
        }
        if clear_value {
            saved_value = Some(access_fn(CounterAccess::ReadValue)?);
        }
        if let Some(event) = event.filter(|_| !temporarily_stop_event) {
            access_fn(CounterAccess::WriteEvent(event))?;
        }
        if clear_value {
            access_fn(CounterAccess::WriteValue(0, write_mode))?;
        }
        if temporarily_stop_event {
            // A legacy programmable counter resumes counting when its selector
            // is restored, regardless of the requested automatic-start mode.
            access_fn(CounterAccess::WriteEvent(
                event.unwrap_or(saved_event.expect("stopped event was saved")),
            ))?;
        }
        if let (CounterStartMode::Start, Some(bits)) = (start_mode, inhibit) {
            access_fn(CounterAccess::WriteInhibit(bits & !inhibit_mask))?;
            if index < 3 {
                // QEMU updates the fixed-counter supervisor shadow after
                // reading its machine counter following an inhibit change.
                access_fn(CounterAccess::ReadValue)?;
            }
        }
        Ok(())
    })();
    if let Err(operation) = result {
        let restore_result = (|| {
            // A failing start may have changed inhibition. Stop the counter
            // before restoring either half of its value.
            if let Some(bits) = inhibit {
                access_fn(CounterAccess::WriteInhibit(bits))?;
            } else if temporarily_stop_event {
                access_fn(CounterAccess::WriteEvent(0))?;
                if access_fn(CounterAccess::ReadEvent)? != 0 {
                    return Err(crate::trap::Error::UnsupportedInstruction);
                }
            }
            if let Some(value) = saved_value {
                access_fn(CounterAccess::WriteValue(value, write_mode))?;
            }
            if let Some(event) = saved_event {
                access_fn(CounterAccess::WriteEvent(event))?;
            }
            if inhibit.is_some() && index < 3 {
                access_fn(CounterAccess::ReadValue)?;
            }
            Ok(())
        })();
        return match restore_result {
            Ok(()) => Err(CounterError::Access(operation)),
            Err(restoration) => Err(CounterError::RestoreFailed {
                operation,
                restoration,
            }),
        };
    }
    Ok(())
}

fn counter_index(mask: u32, position: usize) -> Result<u8, CounterError> {
    if position >= u32::BITS as usize || mask & (1u32 << position) == 0 {
        return Err(CounterError::InvalidCounter);
    }
    if position == 1 {
        return Err(CounterError::TimeCounter);
    }
    Ok(position as u8)
}

#[cfg(test)]
mod tests {
    use super::{
        CounterAccess, CounterError, CounterStartMode, CounterValueUpdate, configure_counter,
        counter_index,
    };
    use crate::trap::Error;

    struct Registers {
        value: u64,
        event: u64,
        inhibit: u32,
        failures: [Option<(CounterAccess, Error)>; 2],
        writes: usize,
    }

    impl Registers {
        fn new() -> Self {
            Self {
                value: 0x1234_5678_9abc_def0,
                event: 7,
                inhibit: 0xffff_fff8,
                failures: [None, None],
                writes: 0,
            }
        }

        fn access(&mut self, operation: CounterAccess) -> Result<u64, Error> {
            let result = match operation {
                CounterAccess::WriteInhibit(bits) => {
                    self.writes += 1;
                    self.inhibit = bits;
                    0
                }
                CounterAccess::ReadEvent => self.event,
                CounterAccess::WriteEvent(event) => {
                    self.writes += 1;
                    self.event = event;
                    0
                }
                CounterAccess::ReadValue => self.value,
                CounterAccess::WriteValue(value, _) => {
                    self.writes += 1;
                    self.value = value;
                    0
                }
            };
            // A recovered CSR fault may occur after the hardware changed state.
            // This models late failures, including a partially committed start.
            if let Some((failed_operation, error)) = self.failures[0] {
                if operation == failed_operation {
                    self.failures[0] = self.failures[1].take();
                    return Err(error);
                }
            }
            Ok(result)
        }
    }

    #[test]
    fn configuration_selects_clears_and_starts_as_one_operation() {
        let mut registers = Registers::new();
        let other_inhibit = registers.inhibit & !(1 << 3);
        assert_eq!(
            configure_counter(
                3,
                Some(registers.inhibit),
                Some(9),
                CounterValueUpdate::Clear,
                CounterStartMode::Start,
                |operation| registers.access(operation),
            ),
            Ok(())
        );
        assert_eq!(registers.event, 9);
        assert_eq!(registers.value, 0);
        assert_eq!(registers.inhibit, other_inhibit);
    }

    #[test]
    fn running_counter_rejects_configuration_without_changing_state() {
        let mut registers = Registers::new();
        registers.inhibit &= !(1 << 3);
        let original = (registers.value, registers.event, registers.inhibit);
        assert_eq!(
            configure_counter(
                3,
                Some(registers.inhibit),
                Some(9),
                CounterValueUpdate::Clear,
                CounterStartMode::Start,
                |operation| registers.access(operation),
            ),
            Err(CounterError::AlreadyStarted)
        );
        assert_eq!(
            (registers.value, registers.event, registers.inhibit),
            original
        );
        assert_eq!(registers.writes, 0);
    }

    #[test]
    fn late_start_failure_restores_value_selector_and_inhibition() {
        let mut registers = Registers::new();
        let original = (registers.value, registers.event, registers.inhibit);
        registers.failures[0] = Some((
            CounterAccess::WriteInhibit(registers.inhibit & !(1 << 3)),
            Error::UnsupportedInstruction,
        ));
        assert_eq!(
            configure_counter(
                3,
                Some(registers.inhibit),
                Some(9),
                CounterValueUpdate::Clear,
                CounterStartMode::Start,
                |operation| registers.access(operation),
            ),
            Err(CounterError::Access(Error::UnsupportedInstruction))
        );
        assert_eq!(
            (registers.value, registers.event, registers.inhibit),
            original
        );
    }

    #[test]
    fn failed_value_write_restores_the_old_selector_and_full_value() {
        let mut registers = Registers::new();
        let original = (registers.value, registers.event, registers.inhibit);
        registers.failures[0] = Some((
            CounterAccess::WriteValue(0, super::csr::CounterWriteMode::Stopped),
            Error::UnsupportedInstruction,
        ));
        assert_eq!(
            configure_counter(
                3,
                Some(registers.inhibit),
                Some(9),
                CounterValueUpdate::Clear,
                CounterStartMode::Preserve,
                |operation| registers.access(operation),
            ),
            Err(CounterError::Access(Error::UnsupportedInstruction))
        );
        assert_eq!(
            (registers.value, registers.event, registers.inhibit),
            original
        );
    }

    #[test]
    fn restoration_failure_keeps_both_error_diagnostics() {
        let mut registers = Registers::new();
        let operation_error = Error::UnsupportedInstruction;
        let restoration_error = Error::MemoryFault { cause: 7, tval: 11 };
        registers.failures = [
            Some((
                CounterAccess::WriteInhibit(registers.inhibit & !(1 << 3)),
                operation_error,
            )),
            Some((
                CounterAccess::WriteValue(registers.value, super::csr::CounterWriteMode::Stopped),
                restoration_error,
            )),
        ];
        assert_eq!(
            configure_counter(
                3,
                Some(registers.inhibit),
                Some(9),
                CounterValueUpdate::Clear,
                CounterStartMode::Start,
                |operation| registers.access(operation),
            ),
            Err(CounterError::RestoreFailed {
                operation: operation_error,
                restoration: restoration_error,
            })
        );
    }

    #[test]
    fn preserving_a_programmable_configuration_keeps_its_selector_and_value() {
        let mut registers = Registers::new();
        let original = (registers.value, registers.event, registers.inhibit);
        assert_eq!(
            configure_counter(
                3,
                Some(registers.inhibit),
                None,
                CounterValueUpdate::Preserve,
                CounterStartMode::Preserve,
                |operation| registers.access(operation),
            ),
            Ok(())
        );
        assert_eq!(
            (registers.value, registers.event, registers.inhibit),
            original
        );
        assert_eq!(registers.writes, 0);
    }

    #[test]
    fn fixed_counter_configuration_never_accesses_an_event_selector() {
        let mut registers = Registers::new();
        registers.inhibit |= 1;
        assert_eq!(
            configure_counter(
                0,
                Some(registers.inhibit),
                None,
                CounterValueUpdate::Clear,
                CounterStartMode::Start,
                |operation| {
                    assert!(!matches!(
                        operation,
                        CounterAccess::ReadEvent | CounterAccess::WriteEvent(_)
                    ));
                    registers.access(operation)
                },
            ),
            Ok(())
        );
        assert_eq!(registers.value, 0);
        assert_eq!(registers.inhibit & 1, 0);
    }

    #[test]
    fn legacy_programmable_clear_stops_then_restores_the_selected_event() {
        let mut registers = Registers::new();
        let event = registers.event;
        assert_eq!(
            configure_counter(
                3,
                None,
                None,
                CounterValueUpdate::Clear,
                CounterStartMode::Preserve,
                |operation| {
                    if matches!(
                        operation,
                        CounterAccess::ReadValue | CounterAccess::WriteValue(_, _)
                    ) {
                        assert_eq!(
                            registers.event, 0,
                            "counter must be stopped during a value update"
                        );
                    }
                    registers.access(operation)
                },
            ),
            Ok(())
        );
        assert_eq!(registers.value, 0);
        assert_eq!(registers.event, event);
    }

    #[test]
    fn legacy_selector_failure_restores_the_stopped_snapshot_and_old_event() {
        let mut registers = Registers::new();
        let original = (registers.value, registers.event, registers.inhibit);
        registers.failures[0] = Some((CounterAccess::WriteEvent(9), Error::UnsupportedInstruction));
        assert_eq!(
            configure_counter(
                3,
                None,
                Some(9),
                CounterValueUpdate::Clear,
                CounterStartMode::Start,
                |operation| registers.access(operation),
            ),
            Err(CounterError::Access(Error::UnsupportedInstruction))
        );
        assert_eq!(
            (registers.value, registers.event, registers.inhibit),
            original
        );
    }

    #[test]
    fn sparse_hardware_lookup_uses_positions_and_rejects_the_time_counter() {
        let mask = 0b111 | 1 << 7 | 1 << 31;
        assert_eq!(counter_index(mask, 0), Ok(0));
        assert_eq!(counter_index(mask, 1), Err(CounterError::TimeCounter));
        assert_eq!(counter_index(mask, 2), Ok(2));
        // Position 7 must not be interpreted as dense ordinal 7 or mapped from ordinal 3.
        assert_eq!(counter_index(mask, 7), Ok(7));
        assert_eq!(counter_index(mask, 3), Err(CounterError::InvalidCounter));
        assert_eq!(counter_index(mask, 31), Ok(31));
        assert_eq!(counter_index(mask, 32), Err(CounterError::InvalidCounter));
        assert_eq!(
            counter_index(mask, usize::MAX),
            Err(CounterError::InvalidCounter)
        );
        assert_eq!(counter_index(0, 0), Err(CounterError::InvalidCounter));
    }
}
