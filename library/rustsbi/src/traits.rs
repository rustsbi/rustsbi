#[cfg(feature = "machine")]
use riscv::register::{marchid, mimpid, mvendorid};
use spec::binary::{HartMask, Physical, SbiRet, SharedPtr};

/// RustSBI environment call handler.
pub trait RustSBI {
    /// Handle supervisor environment call with given parameters and return the `SbiRet` result.
    fn handle_ecall(&self, extension: usize, function: usize, param: [usize; 6]) -> SbiRet;
}

impl<T: RustSBI> RustSBI for &T {
    #[inline(always)]
    fn handle_ecall(&self, extension: usize, function: usize, param: [usize; 6]) -> SbiRet {
        <T as RustSBI>::handle_ecall(self, extension, function, param)
    }
}

/// Machine environment information.
///
/// This trait is useful to build an SBI environment when RustSBI is not run directly on RISC-V machine mode.
pub trait EnvInfo {
    /// Vendor ID for the supervisor environment.
    ///
    /// Provides JEDEC manufacturer ID for the provider of the core.
    fn mvendorid(&self) -> usize;
    /// Architecture ID for the supervisor environment.
    ///
    /// Encodes the base micro-architecture of the hart.
    fn marchid(&self) -> usize;
    /// Implementation ID for the supervisor environment.
    ///
    /// Provides a unique encoding for the version of the processor implementation.
    fn mimpid(&self) -> usize;
}

impl<T: EnvInfo> EnvInfo for &T {
    #[inline(always)]
    fn mvendorid(&self) -> usize {
        <T as EnvInfo>::mvendorid(self)
    }
    #[inline(always)]
    fn marchid(&self) -> usize {
        <T as EnvInfo>::marchid(self)
    }
    #[inline(always)]
    fn mimpid(&self) -> usize {
        <T as EnvInfo>::mimpid(self)
    }
}

// Macro internal structures and functions.
// DO NOT USE code here directly; use derive-macro #[derive(RustSBI)] instead.

#[cfg(feature = "machine")]
#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_base_bare<U: _ExtensionProbe>(
    param: [usize; 6],
    function: usize,
    probe: U,
) -> SbiRet {
    let [param0] = [param[0]];
    let value = match function {
        spec::base::GET_SBI_SPEC_VERSION => (crate::SBI_SPEC_MAJOR << 24) | (crate::SBI_SPEC_MINOR),
        spec::base::GET_SBI_IMPL_ID => crate::IMPL_ID_RUSTSBI,
        spec::base::GET_SBI_IMPL_VERSION => crate::RUSTSBI_VERSION,
        spec::base::PROBE_EXTENSION => probe.probe_extension(param0),
        spec::base::GET_MVENDORID => mvendorid::read().bits(),
        spec::base::GET_MARCHID => marchid::read().bits(),
        spec::base::GET_MIMPID => mimpid::read().bits(),
        _ => return SbiRet::not_supported(),
    };
    SbiRet::success(value)
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_base_env_info<T: EnvInfo, U: _ExtensionProbe>(
    param: [usize; 6],
    function: usize,
    env_info: &T,
    probe: U,
) -> SbiRet {
    let [param0] = [param[0]];
    let value = match function {
        spec::base::GET_SBI_SPEC_VERSION => (crate::SBI_SPEC_MAJOR << 24) | (crate::SBI_SPEC_MINOR),
        spec::base::GET_SBI_IMPL_ID => crate::IMPL_ID_RUSTSBI,
        spec::base::GET_SBI_IMPL_VERSION => crate::RUSTSBI_VERSION,
        spec::base::PROBE_EXTENSION => probe.probe_extension(param0),
        spec::base::GET_MVENDORID => env_info.mvendorid(),
        spec::base::GET_MARCHID => env_info.marchid(),
        spec::base::GET_MIMPID => env_info.mimpid(),
        _ => return SbiRet::not_supported(),
    };
    SbiRet::success(value)
}

// Probe not only standard SBI extensions, but also (reserving for) custom extensions.
// For standard SBI extensions only, the macro would use `_StandardExtensionProbe`;
// for implementation with custom SBI extensions, macro would use a custom structure
// implementing this trait.
pub trait _ExtensionProbe {
    // Implementors are encouraged to add #[inline] hints on this function.
    fn probe_extension(&self, extension: usize) -> usize;
}

#[doc(hidden)]
pub struct _StandardExtensionProbe {
    pub base: usize,
    pub fence: usize,
    pub timer: usize,
    pub ipi: usize,
    pub hsm: usize,
    pub reset: usize,
    pub pmu: usize,
    pub console: usize,
    pub susp: usize,
    pub cppc: usize,
    pub nacl: usize,
    pub sta: usize,
    // NOTE: remember to add to `fn probe_extension` in `impl _ExtensionProbe` as well
}

impl _ExtensionProbe for _StandardExtensionProbe {
    #[inline(always)]
    fn probe_extension(&self, extension: usize) -> usize {
        match extension {
            spec::base::EID_BASE => self.base,
            spec::time::EID_TIME => self.timer,
            spec::spi::EID_SPI => self.ipi,
            spec::rfnc::EID_RFNC => self.fence,
            spec::srst::EID_SRST => self.reset,
            spec::hsm::EID_HSM => self.hsm,
            spec::pmu::EID_PMU => self.pmu,
            spec::dbcn::EID_DBCN => self.console,
            spec::susp::EID_SUSP => self.susp,
            spec::cppc::EID_CPPC => self.cppc,
            spec::nacl::EID_NACL => self.nacl,
            spec::sta::EID_STA => self.sta,
            _ => spec::base::UNAVAILABLE_EXTENSION,
        }
    }
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_fence<T: crate::Fence>(fence: &T, param: [usize; 6], function: usize) -> SbiRet {
    let [param0, param1, param2, param3, param4] =
        [param[0], param[1], param[2], param[3], param[4]];
    let hart_mask = HartMask::from_mask_base(param0, param1);
    match function {
        spec::rfnc::REMOTE_FENCE_I => fence.remote_fence_i(hart_mask),
        spec::rfnc::REMOTE_SFENCE_VMA => fence.remote_sfence_vma(hart_mask, param2, param3),
        spec::rfnc::REMOTE_SFENCE_VMA_ASID => {
            fence.remote_sfence_vma_asid(hart_mask, param2, param3, param4)
        }
        spec::rfnc::REMOTE_HFENCE_GVMA_VMID => {
            fence.remote_hfence_gvma_vmid(hart_mask, param2, param3, param4)
        }
        spec::rfnc::REMOTE_HFENCE_GVMA => fence.remote_hfence_gvma(hart_mask, param2, param3),
        spec::rfnc::REMOTE_HFENCE_VVMA_ASID => {
            fence.remote_hfence_vvma_asid(hart_mask, param2, param3, param4)
        }
        spec::rfnc::REMOTE_HFENCE_VVMA => fence.remote_hfence_vvma(hart_mask, param2, param3),
        _ => SbiRet::not_supported(),
    }
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_timer<T: crate::Timer>(timer: &T, param: [usize; 6], function: usize) -> SbiRet {
    match () {
        #[cfg(target_pointer_width = "64")]
        () => {
            let [param0] = [param[0]];
            match function {
                spec::time::SET_TIMER => {
                    timer.set_timer(param0 as _);
                    SbiRet::success(0)
                }
                _ => SbiRet::not_supported(),
            }
        }
        #[cfg(target_pointer_width = "32")]
        () => {
            let [param0, param1] = [param[0], param[1]];
            match function {
                spec::time::SET_TIMER => {
                    timer.set_timer(concat_u32(param1, param0));
                    SbiRet::success(0)
                }
                _ => SbiRet::not_supported(),
            }
        }
    }
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_ipi<T: crate::Ipi>(ipi: &T, param: [usize; 6], function: usize) -> SbiRet {
    let [param0, param1] = [param[0], param[1]];
    match function {
        spec::spi::SEND_IPI => ipi.send_ipi(HartMask::from_mask_base(param0, param1)),
        _ => SbiRet::not_supported(),
    }
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_hsm<T: crate::Hsm>(hsm: &T, param: [usize; 6], function: usize) -> SbiRet {
    let [param0, param1, param2] = [param[0], param[1], param[2]];
    match function {
        spec::hsm::HART_START => hsm.hart_start(param0, param1, param2),
        spec::hsm::HART_STOP => hsm.hart_stop(),
        spec::hsm::HART_GET_STATUS => hsm.hart_get_status(param0),
        spec::hsm::HART_SUSPEND => {
            if let Ok(suspend_type) = u32::try_from(param0) {
                hsm.hart_suspend(suspend_type, param1, param2)
            } else {
                SbiRet::invalid_param()
            }
        }
        _ => SbiRet::not_supported(),
    }
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_reset<T: crate::Reset>(reset: &T, param: [usize; 6], function: usize) -> SbiRet {
    let [param0, param1] = [param[0], param[1]];
    match function {
        spec::srst::SYSTEM_RESET => match (u32::try_from(param0), u32::try_from(param1)) {
            (Ok(reset_type), Ok(reset_reason)) => reset.system_reset(reset_type, reset_reason),
            (_, _) => SbiRet::invalid_param(),
        },
        _ => SbiRet::not_supported(),
    }
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_pmu<T: crate::Pmu>(pmu: &T, param: [usize; 6], function: usize) -> SbiRet {
    match () {
        #[cfg(target_pointer_width = "64")]
        () => {
            let [param0, param1, param2, param3, param4] =
                [param[0], param[1], param[2], param[3], param[4]];
            match function {
                spec::pmu::NUM_COUNTERS => SbiRet::success(pmu.num_counters()),
                spec::pmu::COUNTER_GET_INFO => pmu.counter_get_info(param0),
                spec::pmu::COUNTER_CONFIG_MATCHING => {
                    pmu.counter_config_matching(param0, param1, param2, param3, param4 as _)
                }
                spec::pmu::COUNTER_START => pmu.counter_start(param0, param1, param2, param3 as _),
                spec::pmu::COUNTER_STOP => pmu.counter_stop(param0, param1, param2),
                spec::pmu::COUNTER_FW_READ => pmu.counter_fw_read(param0),
                spec::pmu::COUNTER_FW_READ_HI => pmu.counter_fw_read_hi(param0),
                spec::pmu::SNAPSHOT_SET_SHMEM => {
                    pmu.snapshot_set_shmem(SharedPtr::new(param0, param1), param2)
                }
                _ => SbiRet::not_supported(),
            }
        }
        #[cfg(target_pointer_width = "32")]
        () => {
            let [param0, param1, param2, param3, param4, param5] =
                [param[0], param[1], param[2], param[3], param[4], param[5]];
            match function {
                spec::pmu::NUM_COUNTERS => SbiRet::success(pmu.num_counters()),
                spec::pmu::COUNTER_GET_INFO => pmu.counter_get_info(param0),
                spec::pmu::COUNTER_CONFIG_MATCHING => pmu.counter_config_matching(
                    param0,
                    param1,
                    param2,
                    param3,
                    concat_u32(param5, param4),
                ),
                spec::pmu::COUNTER_START => {
                    pmu.counter_start(param0, param1, param2, concat_u32(param4, param3))
                }
                spec::pmu::COUNTER_STOP => pmu.counter_stop(param0, param1, param2),
                spec::pmu::COUNTER_FW_READ => pmu.counter_fw_read(param0),
                spec::pmu::COUNTER_FW_READ_HI => pmu.counter_fw_read_hi(param0),
                spec::pmu::SNAPSHOT_SET_SHMEM => {
                    pmu.snapshot_set_shmem(SharedPtr::new(param0, param1), param2)
                }
                _ => SbiRet::not_supported(),
            }
        }
    }
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_console<T: crate::Console>(
    console: &T,
    param: [usize; 6],
    function: usize,
) -> SbiRet {
    let [param0, param1, param2] = [param[0], param[1], param[2]];
    match function {
        spec::dbcn::CONSOLE_WRITE => {
            let bytes = Physical::new(param0, param1, param2);
            console.write(bytes)
        }
        spec::dbcn::CONSOLE_READ => {
            let bytes = Physical::new(param0, param1, param2);
            console.read(bytes)
        }
        spec::dbcn::CONSOLE_WRITE_BYTE => console.write_byte((param0 & 0xFF) as u8),
        _ => SbiRet::not_supported(),
    }
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_susp<T: crate::Susp>(susp: &T, param: [usize; 6], function: usize) -> SbiRet {
    let [param0, param1, param2] = [param[0], param[1], param[2]];
    match function {
        spec::susp::SUSPEND => match u32::try_from(param0) {
            Ok(sleep_type) => susp.system_suspend(sleep_type, param1, param2),
            _ => SbiRet::invalid_param(),
        },
        _ => SbiRet::not_supported(),
    }
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_cppc<T: crate::Cppc>(cppc: &T, param: [usize; 6], function: usize) -> SbiRet {
    match () {
        #[cfg(target_pointer_width = "64")]
        () => {
            let [param0, param1] = [param[0], param[1]];
            match function {
                spec::cppc::PROBE => match u32::try_from(param0) {
                    Ok(reg_id) => cppc.probe(reg_id),
                    _ => SbiRet::invalid_param(),
                },
                spec::cppc::READ => match u32::try_from(param0) {
                    Ok(reg_id) => cppc.read(reg_id),
                    _ => SbiRet::invalid_param(),
                },
                spec::cppc::READ_HI => match u32::try_from(param0) {
                    Ok(reg_id) => cppc.read_hi(reg_id),
                    _ => SbiRet::invalid_param(),
                },
                spec::cppc::WRITE => match u32::try_from(param0) {
                    Ok(reg_id) => cppc.write(reg_id, param1 as _),
                    _ => SbiRet::invalid_param(),
                },
                _ => SbiRet::not_supported(),
            }
        }
        #[cfg(target_pointer_width = "32")]
        () => {
            let [param0, param1, param2] = [param[0], param[1], param[2]];
            match function {
                spec::cppc::PROBE => cppc.probe(param0 as _),
                spec::cppc::READ => cppc.read(param0 as _),
                spec::cppc::READ_HI => cppc.read_hi(param0 as _),
                spec::cppc::WRITE => cppc.write(param0 as _, concat_u32(param2, param1)),
                _ => SbiRet::not_supported(),
            }
        }
    }
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_nacl<T: crate::Nacl>(nacl: &T, param: [usize; 6], function: usize) -> SbiRet {
    let [param0, param1, param2] = [param[0], param[1], param[2]];
    match function {
        spec::nacl::PROBE_FEATURE => match u32::try_from(param0) {
            Ok(feature_id) => nacl.probe_feature(feature_id),
            _ => SbiRet::invalid_param(),
        },
        spec::nacl::SET_SHMEM => nacl.set_shmem(SharedPtr::new(param0, param1), param2),
        spec::nacl::SYNC_CSR => nacl.sync_csr(param0),
        spec::nacl::SYNC_HFENCE => nacl.sync_hfence(param0),
        spec::nacl::SYNC_SRET => nacl.sync_sret(),
        _ => SbiRet::not_supported(),
    }
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_sta<T: crate::Sta>(sta: &T, param: [usize; 6], function: usize) -> SbiRet {
    let [param0, param1, param2] = [param[0], param[1], param[2]];
    match function {
        spec::sta::SET_SHMEM => sta.set_shmem(SharedPtr::new(param0, param1), param2),
        _ => SbiRet::not_supported(),
    }
}

#[cfg(target_pointer_width = "32")]
#[inline]
const fn concat_u32(h: usize, l: usize) -> u64 {
    ((h as u64) << 32) | (l as u64)
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_fence_probe<T: crate::Fence>(fence: &T) -> usize {
    fence._rustsbi_probe()
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_timer_probe<T: crate::Timer>(timer: &T) -> usize {
    timer._rustsbi_probe()
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_ipi_probe<T: crate::Ipi>(ipi: &T) -> usize {
    ipi._rustsbi_probe()
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_hsm_probe<T: crate::Hsm>(hsm: &T) -> usize {
    hsm._rustsbi_probe()
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_reset_probe<T: crate::Reset>(reset: &T) -> usize {
    reset._rustsbi_probe()
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_pmu_probe<T: crate::Pmu>(pmu: &T) -> usize {
    pmu._rustsbi_probe()
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_console_probe<T: crate::Console>(console: &T) -> usize {
    console._rustsbi_probe()
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_susp_probe<T: crate::Susp>(susp: &T) -> usize {
    susp._rustsbi_probe()
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_cppc_probe<T: crate::Cppc>(cppc: &T) -> usize {
    cppc._rustsbi_probe()
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_nacl_probe<T: crate::Nacl>(nacl: &T) -> usize {
    nacl._rustsbi_probe()
}

#[doc(hidden)]
#[inline(always)]
pub fn _rustsbi_sta_probe<T: crate::Sta>(sta: &T) -> usize {
    sta._rustsbi_probe()
}
