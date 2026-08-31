//! T-Head C900-compatible Core Local Interruptor (CLINT).

use crate::cfg::NUM_HART_MAX;
use crate::driver::{IpiSender, TimerDevice};
use crate::platform::mmio::Mmio;

/// Register span through the configured harts' timer comparison registers.
pub(super) const SPAN: usize = 0x4000 + NUM_HART_MAX * 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Half {
    Low,
    High,
}

#[derive(Clone, Copy)]
enum Reg {
    Msip,
    MtimecmpLow,
    MtimecmpHigh,
}

impl Reg {
    fn offset(self, hart: usize) -> usize {
        match self {
            Self::Msip => hart * 4,
            Self::MtimecmpLow => 0x4000 + hart * 8,
            Self::MtimecmpHigh => 0x4000 + hart * 8 + 4,
        }
    }
}

// T-Head CLINTs have no 64-bit MMIO path, so `mtimecmp` is accessed as two
// 32-bit halves. Re-read the high half around the low read and retry until
// it is stable, so a timer tick between the reads cannot tear the value.
fn read_split(mut read_half: impl FnMut(Half) -> u32) -> u64 {
    loop {
        let high = read_half(Half::High);
        let low = read_half(Half::Low);
        if read_half(Half::High) == high {
            return (u64::from(high) << 32) | u64::from(low);
        }
    }
}

// Writing the halves in rising order could transiently hold a compare
// value below `mtime` and fire a spurious interrupt; writing the maximum
// low half first keeps every intermediate value above any real deadline.
fn write_split(value: u64, mut write_half: impl FnMut(Half, u32)) {
    write_half(Half::Low, u32::MAX);
    write_half(Half::High, (value >> 32) as u32);
    write_half(Half::Low, value as u32);
}

/// T-Head CLINT using split 32-bit timer comparison registers.
pub(super) struct THeadClint {
    mmio: Mmio,
}

impl THeadClint {
    /// Wraps an acquired register block.
    pub(super) fn new(mmio: Mmio) -> Self {
        Self { mmio }
    }

    fn read_mtimecmp(&self, hart_idx: usize) -> u64 {
        read_split(|half| {
            let reg = match half {
                Half::Low => Reg::MtimecmpLow,
                Half::High => Reg::MtimecmpHigh,
            };
            self.mmio.read::<u32>(reg.offset(hart_idx))
        })
    }
}

impl TimerDevice for THeadClint {
    #[inline(always)]
    fn read_time(&self) -> u64 {
        // T-Head CLINTs have no memory-mapped `mtime`; read the `time` CSR.
        riscv::register::time::read64()
    }

    #[inline(always)]
    fn set_timer(&self, hart_idx: usize, value: u64) {
        if self.read_mtimecmp(hart_idx) == value {
            return;
        }
        write_split(value, |half, word| {
            let reg = match half {
                Half::Low => Reg::MtimecmpLow,
                Half::High => Reg::MtimecmpHigh,
            };
            self.mmio.write::<u32>(reg.offset(hart_idx), word);
        });
    }
}

impl IpiSender for THeadClint {
    #[inline(always)]
    fn send_ipi(&self, hart_idx: usize) {
        self.mmio.write::<u32>(Reg::Msip.offset(hart_idx), 1)
    }

    #[inline(always)]
    fn clear_ipi(&self) {
        self.mmio
            .write::<u32>(Reg::Msip.offset(crate::riscv::current_hartid()), 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_access_is_tear_safe() {
        let mut reads = [
            (Half::High, 1),
            (Half::Low, u32::MAX),
            (Half::High, 2),
            (Half::High, 2),
            (Half::Low, 3),
            (Half::High, 2),
        ]
        .into_iter();
        let value = read_split(|half| {
            let (expected, word) = reads.next().unwrap();
            assert_eq!(half, expected);
            word
        });
        assert_eq!(value, 0x0000_0002_0000_0003);
        assert!(reads.next().is_none());

        let mut writes = alloc::vec::Vec::new();
        write_split(0x1122_3344_5566_7788, |half, word| {
            writes.push((half, word))
        });
        assert_eq!(
            writes,
            [
                (Half::Low, u32::MAX),
                (Half::High, 0x1122_3344),
                (Half::Low, 0x5566_7788),
            ]
        );
    }
}
