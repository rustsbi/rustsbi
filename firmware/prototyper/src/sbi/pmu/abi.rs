//! SBI parameter decoding and return-value encoding.

use super::{SbiPmu, event::Event};
use runtime::{
    Error,
    rustsbi::{Pmu as SbiPmuExtension, SbiRet},
};
use sbi_spec::{
    binary::SharedPtr,
    pmu::{
        flags::{ConfigFlags, StartFlags, StopFlags},
        shmem_size::SIZE,
    },
};

impl SbiPmuExtension for SbiPmu {
    fn _rustsbi_probe(&self) -> usize {
        usize::from(self.pmu.is_available())
    }

    fn num_counters(&self) -> usize {
        self.with_current(|_, counters| Ok(counters.count()))
            .unwrap_or(0)
    }

    fn counter_get_info(&self, index: usize) -> SbiRet {
        self.with_current(|hardware, counters| counters.info(index, hardware))
            .map_or_else(Into::into, SbiRet::success)
    }

    fn counter_config_matching(
        &self,
        first: usize,
        offsets: usize,
        flags: usize,
        event: usize,
        data: u64,
    ) -> SbiRet {
        let result = (|| {
            let flags = ConfigFlags::from_bits(flags).ok_or(Error::InvalidArgs)?;
            self.with_current(|hardware, counters| {
                let selected = counters.select(first, offsets)?;
                counters.configure(
                    selected,
                    flags,
                    Event::try_from(event)?,
                    data,
                    &self.events,
                    hardware,
                )
            })
        })();
        result.map_or_else(Into::into, SbiRet::success)
    }

    fn counter_start(&self, first: usize, offsets: usize, flags: usize, initial: u64) -> SbiRet {
        StartFlags::from_bits(flags)
            .ok_or(Error::InvalidArgs)
            .and_then(|flags| {
                self.with_current(|hardware, counters| {
                    let selected = counters.select(first, offsets)?;
                    if flags.contains(StartFlags::INIT_VALUE)
                        && (offsets.count_ones() != 1 || flags.contains(StartFlags::INIT_SNAPSHOT))
                    {
                        return Err(Error::InvalidArgs);
                    }
                    if flags.contains(StartFlags::INIT_SNAPSHOT) {
                        return Err(Error::NoSharedMemory);
                    }
                    counters.start(
                        selected,
                        flags.contains(StartFlags::INIT_VALUE).then_some(initial),
                        hardware,
                    )
                })
            })
            .map_or_else(Into::into, |()| SbiRet::success(0))
    }

    fn counter_stop(&self, first: usize, offsets: usize, flags: usize) -> SbiRet {
        StopFlags::from_bits(flags)
            .ok_or(Error::InvalidArgs)
            .and_then(|flags| {
                self.with_current(|hardware, counters| {
                    let selected = counters.select(first, offsets)?;
                    if flags.contains(StopFlags::TAKE_SNAPSHOT) {
                        return Err(Error::NoSharedMemory);
                    }
                    counters.stop(selected, flags, hardware)
                })
            })
            .map_or_else(Into::into, |()| SbiRet::success(0))
    }

    fn counter_fw_read(&self, index: usize) -> SbiRet {
        self.with_current(|_, counters| counters.read_firmware(index))
            .map_or_else(Into::into, |value| SbiRet::success(value as usize))
    }

    fn counter_fw_read_hi(&self, index: usize) -> SbiRet {
        self.with_current(|_, counters| counters.read_firmware(index))
            .map_or_else(Into::into, |value| {
                SbiRet::success(if cfg!(target_pointer_width = "32") {
                    (value >> 32) as usize
                } else {
                    0
                })
            })
    }

    fn snapshot_set_shmem(&self, _: SharedPtr<[u8; SIZE]>, _: usize) -> SbiRet {
        SbiRet::not_supported()
    }
}
