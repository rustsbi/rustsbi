//! SBI PMU logical-counter numbering.
//!
//! A hardware counter's logical index is its CSR offset from `cycle`: index 0
//! is `cycle`, index 2 is `instret`, and index N is `hpmcounterN`. Missing
//! counters leave holes; they are not renumbered. Firmware counters begin
//! immediately after the highest implemented hardware index.
//!
//! The supervisor-visible counter range starts at CSR `0xC00`. This module has
//! no firmware dependencies so the numbering can be tested on the host.

pub(super) const CSR_CYCLE: u16 = 0xc00;

/// Hardware slots reserved by the privileged architecture, including `time`.
pub(super) const HARDWARE_COUNTER_SLOTS: usize = 32;
/// Firmware counters published after the hardware CSR range.
pub(super) const FIRMWARE_COUNTER_MAX: usize = 16;

const TIME_OFFSET: u16 = 1;

/// CSR and encoded width reported for one implemented hardware counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HardwareCounterInfo {
    pub csr: u16,
    pub width_minus_one: u8,
}

/// Maps a hart's implemented-counter bitmap onto SBI logical indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CounterMap {
    /// Bit N is set when CSR offset N (`cycle` + N) is implemented.
    mask: u32,
    /// Encoded width (`width - 1`) of programmable counters 3..=31.
    hpm_width_minus_one: u8,
}

impl CounterMap {
    /// Builds a map from the probed `mhpm` bitmap.
    ///
    /// Bits above 31 are ignored. `mhpm_bits` is the programmable-counter
    /// width; `cycle` and `instret` stay 64 bits. A zero width is treated as
    /// the 64-bit width Prototyper currently probes.
    pub(super) const fn new(mask: u32, mhpm_bits: u32) -> Self {
        let width = if mhpm_bits == 0 { 64 } else { mhpm_bits };
        let mut encoded = width.saturating_sub(1);
        if encoded > 63 {
            encoded = 63;
        }
        Self {
            mask,
            hpm_width_minus_one: encoded as u8,
        }
    }

    /// Highest hardware index plus one.
    ///
    /// This is the CSR span, not the number of implemented counters. With no
    /// programmable counter the span is still 3, so indices 0 and 2 remain
    /// `cycle` and `instret`.
    pub(super) const fn hardware_span(self) -> usize {
        match 32 - self.mask.leading_zeros() {
            0 => 3,
            span => span as usize,
        }
    }

    /// `sbi_pmu_num_counters`: hardware span plus the firmware counters.
    pub(super) const fn num_counters(self) -> usize {
        self.hardware_span() + FIRMWARE_COUNTER_MAX
    }

    /// Returns whether `counter_idx` names a firmware counter.
    pub(super) const fn is_firmware(self, counter_idx: usize) -> bool {
        counter_idx >= self.hardware_span() && counter_idx < self.num_counters()
    }

    /// Converts a firmware logical index to its slot in the firmware array.
    pub(super) const fn firmware_slot(self, counter_idx: usize) -> Option<usize> {
        if self.is_firmware(counter_idx) {
            Some(counter_idx - self.hardware_span())
        } else {
            None
        }
    }

    /// Returns the CSR and width for an implemented hardware counter.
    ///
    /// `time` and unimplemented offsets are rejected. The index is not
    /// compressed onto the set bits of `mask`.
    pub(super) const fn hardware_info(self, counter_idx: usize) -> Option<HardwareCounterInfo> {
        if counter_idx >= self.hardware_span() || counter_idx > 31 {
            return None;
        }
        let offset = counter_idx as u16;
        if offset == TIME_OFFSET || (self.mask >> offset) & 1 == 0 {
            return None;
        }
        let width_minus_one = if offset == 0 || offset == 2 {
            63
        } else {
            self.hpm_width_minus_one
        };
        Some(HardwareCounterInfo {
            csr: CSR_CYCLE + offset,
            width_minus_one,
        })
    }

    /// Returns the machine-counter offset of a startable hardware counter.
    ///
    /// The offset equals the logical index. `time` is not startable.
    pub(super) const fn hardware_offset(self, counter_idx: usize) -> Option<u16> {
        match self.hardware_info(counter_idx) {
            Some(info) => Some(info.csr - CSR_CYCLE),
            None => None,
        }
    }
}
