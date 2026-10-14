//! Machine performance counters.
use crate::value::Value;
use crate::values::counter::CounterValue;
use crate::{Readable, Writeable};

/// <https://docs.riscv.org/reference/isa/priv/machine.html#_hardware_performance_monitor>
pub enum PerformanceCounter<const INDEX: u8> {}

impl<const INDEX: u8> Readable for PerformanceCounter<INDEX> {
    const NUMBER: u16 = {
        assert!(
            INDEX >= 3 && INDEX <= 31,
            "performance counter index must be 3..=31"
        );
        0xb00 + INDEX as u16
    };
    type Value = CounterValue;
    fn decode(bits: usize) -> Self::Value {
        CounterValue::from_bits(bits)
    }
}

impl<const INDEX: u8> Writeable for PerformanceCounter<INDEX> {
    const NUMBER: u16 = <Self as Readable>::NUMBER;
    type Value = CounterValue;
    fn encode(value: Self::Value) -> usize {
        value.bits()
    }
}
