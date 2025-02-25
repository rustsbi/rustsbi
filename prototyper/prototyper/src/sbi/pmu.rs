use crate::{riscv::current_hartid, sbi::features::hart_mhpm_mask};
use rustsbi::{Pmu, SbiRet};
use sbi_spec::{binary::SharedPtr, pmu::shmem_size::SIZE};

use super::trap_stack::hart_context;

const HARDWARE_COUNTER_MAX: usize = 32;
const FIRMWARE_COUNTER_MAX: usize = 16;

pub struct PmuState {
    active_event: [i64; HARDWARE_COUNTER_MAX + FIRMWARE_COUNTER_MAX],
    fw_counter_state: usize,
    fw_counter: [i64; FIRMWARE_COUNTER_MAX],
}

struct SbiPmu;

impl Pmu for SbiPmu {
    fn num_counters(&self) -> usize {
        hart_mhpm_mask(current_hartid()).count_ones() as usize + FIRMWARE_COUNTER_MAX
    }

    fn counter_get_info(&self, counter_idx: usize) -> SbiRet {
        todo!()
    }

    fn counter_config_matching(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        config_flags: usize,
        event_idx: usize,
        event_data: u64,
    ) -> SbiRet {
        todo!()
    }

    fn counter_start(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        start_flags: usize,
        initial_value: u64,
    ) -> SbiRet {
        todo!()
    }

    fn counter_stop(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        stop_flags: usize,
    ) -> SbiRet {
        todo!()
    }

    fn counter_fw_read(&self, counter_idx: usize) -> SbiRet {
        todo!()
    }

    fn counter_fw_read_hi(&self, counter_idx: usize) -> SbiRet {
        todo!()
    }

    fn snapshot_set_shmem(&self, shmem: SharedPtr<[u8; SIZE]>, flags: usize) -> SbiRet {
        // Optional function, `not_supported` is returned if not implemented.
        let _ = (shmem, flags);
        SbiRet::not_supported()
    }
}
