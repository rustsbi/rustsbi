use rustsbi::{Pmu, SbiRet};
use sbi_spec::binary::SharedPtr;
use sbi_spec::pmu::shmem_size::SIZE;
use sbi_spec::pmu::*;

use crate::riscv::csr::CSR_CYCLE;
use crate::{riscv::current_hartid, sbi::features::hart_mhpm_mask};

use super::trap_stack::hart_context;

const HARDWARE_COUNTER_MAX: usize = 32;
const FIRMWARE_COUNTER_MAX: usize = 16;

/// PMU activation event and firmware counters
pub struct PmuState {
    active_event: [usize; HARDWARE_COUNTER_MAX + FIRMWARE_COUNTER_MAX],
    // Firmware counter status mask, each bit represents a firmware counter.
    // A bit set to 1 indicates that the corresponding firmware counter starts counting
    fw_counter_state: usize,
    fw_counter: [u64; FIRMWARE_COUNTER_MAX],
}

impl PmuState {
    pub fn new() -> Self {
        Self {
            active_event: [0; HARDWARE_COUNTER_MAX + FIRMWARE_COUNTER_MAX],
            fw_counter_state: 0,
            fw_counter: [0; FIRMWARE_COUNTER_MAX],
        }
    }

    pub fn get_event_idx(&self, counter_idx: usize, firmware_event: bool) -> Option<EventIdx> {
        let mhpm_mask = hart_mhpm_mask(current_hartid());
        let hw_counters_num = mhpm_mask.count_ones() as usize;
        let total_counters_num = hw_counters_num + FIRMWARE_COUNTER_MAX;
        if counter_idx >= total_counters_num {
            return None;
        }
        if firmware_event && counter_idx <= hw_counters_num {
            return None;
        }
        return Some(EventIdx::new(self.active_event[counter_idx]));
    }

    pub fn get_fw_counter(&self, counter_idx: usize) -> u64 {
        // TODO: maybe need to check the validity of counter_idx
        let hw_counters_num = hart_mhpm_mask(current_hartid()).count_ones() as usize;
        return self.fw_counter[counter_idx - hw_counters_num];
    }
}

struct SbiPmu;

fn get_hpm_csr_offset(counter_idx: usize, mhpm_mask: u32) -> Option<u16> {
    let mut count = 0;
    for offset in 0..32 {
        if (mhpm_mask >> offset) & 1 == 1 {
            if count == counter_idx {
                return Some(offset as u16);
            }
            count += 1;
        }
    }
    None
}

impl Pmu for SbiPmu {
    fn num_counters(&self) -> usize {
        hart_mhpm_mask(current_hartid()).count_ones() as usize + FIRMWARE_COUNTER_MAX
    }

    fn counter_get_info(&self, counter_idx: usize) -> SbiRet {
        let mhpm_mask = hart_mhpm_mask(current_hartid());
        let hw_counters_num = mhpm_mask.count_ones() as usize;
        let total_counters_num = hw_counters_num + FIRMWARE_COUNTER_MAX;
        let mut counter_info = CounterInfo::default();
        if counter_idx >= total_counters_num {
            return SbiRet::invalid_param();
        }

        if counter_idx < hw_counters_num {
            let o_csr_offset = get_hpm_csr_offset(counter_idx, mhpm_mask);
            if let Some(csr_offest) = o_csr_offset {
                counter_info.set_hardware_info(CSR_CYCLE + csr_offest, 63);
            } else {
                return SbiRet::invalid_param();
            }
        } else {
            counter_info.set_firmware_info();
        }

        SbiRet::success(counter_info.inner)
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
        let o_event_id = hart_context(current_hartid())
            .pmu_state
            .get_event_idx(counter_idx, true);
        if let Some(event_id) = o_event_id {
            if !event_id.firmware_event_validate() {
                return SbiRet::invalid_param();
            }

            if event_id.event_code() == firmware_event::PLATFORM {
                // TODO: Platform PMU events need to be handled here
                return SbiRet::invalid_param();
            }

            let fw_counter_value = hart_context(current_hartid())
                .pmu_state
                .get_fw_counter(counter_idx) as usize;
            return SbiRet::success(fw_counter_value);
        } else {
            return SbiRet::invalid_param();
        }
    }

    fn counter_fw_read_hi(&self, _counter_idx: usize) -> SbiRet {
        // The Specification states the this function  always returns zero in sbiret.value for RV64 (or higher) systems.
        // Currently RustSBI Prototyper only supports RV64 systems
        SbiRet::success(0)
    }

    fn snapshot_set_shmem(&self, shmem: SharedPtr<[u8; SIZE]>, flags: usize) -> SbiRet {
        // Optional function, `not_supported` is returned if not implemented.
        let _ = (shmem, flags);
        SbiRet::not_supported()
    }
}

struct CounterInfo {
    inner: usize,
}

impl Default for CounterInfo {
    fn default() -> Self {
        Self { inner: 0 }
    }
}

impl CounterInfo {
    fn set_csr(&mut self, csr_num: u16) {
        self.inner = (self.inner & !0xFFF) | (csr_num as usize & 0xFFF);
    }

    fn set_width(&mut self, width: u8) {
        self.inner = (self.inner & !(0x3F << 12)) | ((width as usize & 0x3F) << 12);
    }

    fn set_hardware_info(&mut self, csr_num: u16, width: u8) {
        self.inner = 0;
        self.set_csr(csr_num);
        self.set_width(width);
    }

    fn set_firmware_info(&mut self) {
        self.inner = 1 << (size_of::<usize>() - 1);
    }
}

struct EventToCounterMap {
    counters_mask: u32,
    event_start_idx: u32,
    event_end_id: u32,
}

struct RawEventToCounterMap {
    counters_mask: u32,
    raw_event_select: u64,
    select_mask: u64,
}

struct EventIdx {
    inner: usize,
}

impl EventIdx {
    fn new(event_idx: usize) -> Self {
        Self { inner: event_idx }
    }

    fn event_type(&self) -> usize {
        (self.inner >> 16) & 0xF
    }

    fn event_code(&self) -> usize {
        self.inner & 0xFFFF
    }

    fn cache_id(&self) -> usize {
        (self.inner >> 3) & 0x1FFF
    }

    fn cache_op_id(&self) -> usize {
        (self.inner >> 1) & 0x3
    }

    fn cache_result_id(&self) -> usize {
        self.inner & 0x1
    }

    fn validate(&self) -> bool {
        let event_type = self.event_type();
        let event_code = self.event_code();
        match event_type {
            event_type::HARDWARE_GENERAL => {
                if event_code > hardware_event::REF_CPU_CYCLES {
                    return false;
                }
            }
            event_type::HARDWARE_CACHE => {
                let cache_id = self.cache_id();
                let cache_op_id = self.cache_op_id();
                let cache_result_id = self.cache_result_id();
                if cache_id > cache_event::NODE
                    || cache_op_id > cache_operation::PREFETCH
                    || cache_result_id > cache_result::MISS
                {
                    return false;
                }
            }
            event_type::HARDWARE_RAW | event_type::HARDWARE_RAW_V2 => {
                if event_code != 0 {
                    return false;
                }
            }
            event_type::FIRMWARE => {
                if (event_code > firmware_event::HFENCE_VVMA_ASID_RECEIVED
                    && event_code < firmware_event::PLATFORM)
                    || event_code > firmware_event::PLATFORM
                {
                    return false;
                }
                if event_code == firmware_event::PLATFORM {
                    // TODO: should check platform's pmu config
                    return false;
                }
            }
            _ => {
                return false;
            }
        }
        true
    }

    fn firmware_event_validate(&self) -> bool {
        let event_type = self.event_type();
        let event_code = self.event_code();
        if event_type != event_type::FIRMWARE {
            return false;
        }
        if (event_code > firmware_event::HFENCE_VVMA_ASID_RECEIVED
            && event_code < firmware_event::PLATFORM)
            || event_code > firmware_event::PLATFORM
        {
            return false;
        }
        true
    }
}
