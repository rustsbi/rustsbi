use crate::{
    spec::binary::SbiRet, Fence, HartMask, Hsm, Ipi, Pmu, Reset, Timer, IMPL_ID_RUSTSBI,
    RUSTSBI_VERSION, SBI_SPEC_MAJOR, SBI_SPEC_MINOR,
};
use core::convert::Infallible;
#[cfg(feature = "machine")]
use riscv::register::{marchid, mimpid, mvendorid};

/// RustSBI instance including standard extensions
///
/// By now RustSBI supports to run instance based interface on systems has environment pointer width
/// that is the same as supervisor pointer width.
#[derive(Clone, Debug)]
pub struct RustSBI<T, I, R, H, S, P> {
    timer: Option<T>,
    ipi: Option<I>,
    rfnc: Option<R>,
    hsm: Option<H>,
    srst: Option<S>,
    pmu: Option<P>,
    #[cfg(not(feature = "machine"))]
    info: MachineInfo,
}

/// Machine information for SBI environment
///
/// This structure is useful to build an SBI environment when RustSBI is not run directly on RISC-V machine mode.
#[cfg(not(feature = "machine"))]
#[derive(Clone, Copy, Debug)]
pub struct MachineInfo {
    /// Register `mvendorid` for supervisor environment
    pub mvendorid: usize,
    /// Register `marchid` for supervisor environment
    pub marchid: usize,
    /// Register `mimpid` for supervisor environment
    pub mimpid: usize,
}

impl<T: Timer, I: Ipi, R: Fence, H: Hsm, S: Reset, P: Pmu> RustSBI<T, I, R, H, S, P> {
    /// Create RustSBI instance on current machine environment for all the SBI extensions
    #[cfg(feature = "machine")]
    #[inline]
    pub const fn new_machine(timer: T, ipi: I, rfnc: R, hsm: H, srst: S, pmu: P) -> Self {
        Self {
            timer: Some(timer),
            ipi: Some(ipi),
            rfnc: Some(rfnc),
            hsm: Some(hsm),
            srst: Some(srst),
            pmu: Some(pmu),
        }
    }

    /// Create RustSBI instance on given machine information for all the SBI extensions
    #[cfg(not(feature = "machine"))]
    #[inline]
    pub const fn with_machine_info(
        timer: T,
        ipi: I,
        rfnc: R,
        hsm: H,
        srst: S,
        pmu: P,
        info: MachineInfo,
    ) -> Self {
        Self {
            timer: Some(timer),
            ipi: Some(ipi),
            rfnc: Some(rfnc),
            hsm: Some(hsm),
            srst: Some(srst),
            pmu: Some(pmu),
            info,
        }
    }

    /// Handle supervisor environment call with given parameters and return the `SbiRet` result.
    #[inline]
    pub fn handle_ecall(&mut self, extension: usize, function: usize, param: [usize; 6]) -> SbiRet {
        match extension {
            spec::rfnc::EID_RFNC => {
                let rfnc = if let Some(rfnc) = &mut self.rfnc {
                    rfnc
                } else {
                    return SbiRet::not_supported();
                };
                let [param0, param1, param2, param3, param4] =
                    [param[0], param[1], param[2], param[3], param[4]];
                let hart_mask = crate::HartMask::from_mask_base(param0, param1);
                match function {
                    spec::rfnc::REMOTE_FENCE_I => rfnc.remote_fence_i(hart_mask),
                    spec::rfnc::REMOTE_SFENCE_VMA => {
                        rfnc.remote_sfence_vma(hart_mask, param2, param3)
                    }
                    spec::rfnc::REMOTE_SFENCE_VMA_ASID => {
                        rfnc.remote_sfence_vma_asid(hart_mask, param2, param3, param4)
                    }
                    spec::rfnc::REMOTE_HFENCE_GVMA_VMID => {
                        rfnc.remote_hfence_gvma_vmid(hart_mask, param2, param3, param4)
                    }
                    spec::rfnc::REMOTE_HFENCE_GVMA => {
                        rfnc.remote_hfence_gvma(hart_mask, param2, param3)
                    }
                    spec::rfnc::REMOTE_HFENCE_VVMA_ASID => {
                        rfnc.remote_hfence_vvma_asid(hart_mask, param2, param3, param4)
                    }
                    spec::rfnc::REMOTE_HFENCE_VVMA => {
                        rfnc.remote_hfence_vvma(hart_mask, param2, param3)
                    }
                    _ => SbiRet::not_supported(),
                }
            }
            spec::time::EID_TIME => match () {
                #[cfg(target_pointer_width = "64")]
                () => {
                    let timer = if let Some(timer) = &mut self.timer {
                        timer
                    } else {
                        return SbiRet::not_supported();
                    };
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
                    let timer = if let Some(timer) = &mut self.timer {
                        timer
                    } else {
                        return SbiRet::not_supported();
                    };
                    let [param0, param1] = [param[0], param[1]];
                    match function {
                        spec::time::SET_TIMER => {
                            timer.set_timer(concat_u32(param1, param0));
                            SbiRet::success(0)
                        }
                        _ => SbiRet::not_supported(),
                    }
                }
            },
            spec::spi::EID_SPI => {
                let ipi = if let Some(ipi) = &mut self.ipi {
                    ipi
                } else {
                    return SbiRet::not_supported();
                };
                let [param0, param1] = [param[0], param[1]];
                match function {
                    spec::spi::SEND_IPI => ipi.send_ipi(HartMask::from_mask_base(param0, param1)),
                    _ => SbiRet::not_supported(),
                }
            }
            spec::base::EID_BASE => {
                let [param0] = [param[0]];
                let value = match function {
                    spec::base::GET_SBI_SPEC_VERSION => (SBI_SPEC_MAJOR << 24) | (SBI_SPEC_MINOR),
                    spec::base::GET_SBI_IMPL_ID => IMPL_ID_RUSTSBI,
                    spec::base::GET_SBI_IMPL_VERSION => RUSTSBI_VERSION,
                    spec::base::PROBE_EXTENSION => {
                        // only provides probes to standard extensions. If you have customized extensions to be probed,
                        // run it even before this `handle_ecall` function.
                        self.probe_extension(param0)
                    }
                    spec::base::GET_MVENDORID => match () {
                        #[cfg(feature = "machine")]
                        () => mvendorid::read().map(|r| r.bits()).unwrap_or(0),
                        #[cfg(not(feature = "machine"))]
                        () => self.info.mvendorid,
                    },
                    spec::base::GET_MARCHID => match () {
                        #[cfg(feature = "machine")]
                        () => marchid::read().map(|r| r.bits()).unwrap_or(0),
                        #[cfg(not(feature = "machine"))]
                        () => self.info.marchid,
                    },
                    spec::base::GET_MIMPID => match () {
                        #[cfg(feature = "machine")]
                        () => mimpid::read().map(|r| r.bits()).unwrap_or(0),
                        #[cfg(not(feature = "machine"))]
                        () => self.info.mimpid,
                    },
                    _ => return SbiRet::not_supported(),
                };
                SbiRet::success(value)
            }
            spec::hsm::EID_HSM => {
                let hsm = if let Some(hsm) = &mut self.hsm {
                    hsm
                } else {
                    return SbiRet::not_supported();
                };
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
            spec::srst::EID_SRST => {
                let srst = if let Some(srst) = &mut self.srst {
                    srst
                } else {
                    return SbiRet::not_supported();
                };
                let [param0, param1] = [param[0], param[1]];
                match function {
                    spec::srst::SYSTEM_RESET => {
                        match (u32::try_from(param0), u32::try_from(param1)) {
                            (Ok(reset_type), Ok(reset_reason)) => {
                                srst.system_reset(reset_type, reset_reason)
                            }
                            (_, _) => SbiRet::invalid_param(),
                        }
                    }
                    _ => SbiRet::not_supported(),
                }
            }
            spec::pmu::EID_PMU => match () {
                #[cfg(target_pointer_width = "64")]
                () => {
                    let pmu = if let Some(pmu) = &mut self.pmu {
                        pmu
                    } else {
                        return SbiRet::not_supported();
                    };
                    let [param0, param1, param2, param3, param4] =
                        [param[0], param[1], param[2], param[3], param[4]];
                    match function {
                        spec::pmu::PMU_NUM_COUNTERS => SbiRet::success(pmu.num_counters()),
                        spec::pmu::PMU_COUNTER_GET_INFO => pmu.counter_get_info(param0),
                        spec::pmu::PMU_COUNTER_CONFIG_MATCHING => {
                            pmu.counter_config_matching(param0, param1, param2, param3, param4 as _)
                        }
                        spec::pmu::PMU_COUNTER_START => {
                            pmu.counter_start(param0, param1, param2, param3 as _)
                        }
                        spec::pmu::PMU_COUNTER_STOP => pmu.counter_stop(param0, param1, param2),
                        spec::pmu::PMU_COUNTER_FW_READ => pmu.counter_fw_read(param0),
                        _ => SbiRet::not_supported(),
                    }
                }
                #[cfg(target_pointer_width = "32")]
                () => {
                    let pmu = if let Some(pmu) = &mut self.pmu {
                        pmu
                    } else {
                        return SbiRet::not_supported();
                    };
                    let [param0, param1, param2, param3, param4, param5] =
                        [param[0], param[1], param[2], param[3], param[4], param[5]];
                    match function {
                        spec::pmu::PMU_NUM_COUNTERS => SbiRet::success(pmu.num_counters()),
                        spec::pmu::PMU_COUNTER_GET_INFO => pmu.counter_get_info(param0),
                        spec::pmu::PMU_COUNTER_CONFIG_MATCHING => pmu.counter_config_matching(
                            param0,
                            param1,
                            param2,
                            param3,
                            concat_u32(param5, param4),
                        ),
                        spec::pmu::PMU_COUNTER_START => {
                            pmu.counter_start(param0, param1, param2, concat_u32(param4, param3))
                        }
                        spec::pmu::PMU_COUNTER_STOP => pmu.counter_stop(param0, param1, param2),
                        spec::pmu::PMU_COUNTER_FW_READ => pmu.counter_fw_read(param0),
                        _ => SbiRet::not_supported(),
                    }
                }
            },
            _ => SbiRet::not_supported(),
        }
    }

    #[inline]
    fn probe_extension(&self, extension: usize) -> usize {
        let ans = match extension {
            spec::base::EID_BASE => true,
            spec::time::EID_TIME => self.timer.is_some(),
            spec::spi::EID_SPI => self.ipi.is_some(),
            spec::rfnc::EID_RFNC => self.rfnc.is_some(),
            spec::srst::EID_SRST => self.srst.is_some(),
            spec::hsm::EID_HSM => self.hsm.is_some(),
            spec::pmu::EID_PMU => self.pmu.is_some(),
            _ => false,
        };
        if ans {
            spec::base::UNAVAILABLE_EXTENSION.wrapping_add(1)
        } else {
            spec::base::UNAVAILABLE_EXTENSION
        }
    }
}

#[cfg(target_pointer_width = "32")]
#[inline]
const fn concat_u32(h: usize, l: usize) -> u64 {
    ((h as u64) << 32) | (l as u64)
}

/// Structure to build a RustSBI instance
pub struct Builder<T, I, R, H, S, P> {
    inner: RustSBI<T, I, R, H, S, P>,
}

impl Builder<Infallible, Infallible, Infallible, Infallible, Infallible, Infallible> {
    /// Create a new `Builder` from current machine environment
    #[inline]
    #[cfg(feature = "machine")]
    pub const fn new_machine(
    ) -> Builder<Infallible, Infallible, Infallible, Infallible, Infallible, Infallible> {
        Builder {
            inner: RustSBI {
                timer: None,
                ipi: None,
                rfnc: None,
                hsm: None,
                srst: None,
                pmu: None,
            },
        }
    }

    /// Create a new `Builder` from machine information
    #[inline]
    #[cfg(not(feature = "machine"))]
    pub const fn with_machine_info(
        info: MachineInfo,
    ) -> Builder<Infallible, Infallible, Infallible, Infallible, Infallible, Infallible> {
        Builder {
            inner: RustSBI {
                timer: None,
                ipi: None,
                rfnc: None,
                hsm: None,
                srst: None,
                pmu: None,
                info,
            },
        }
    }
}

// fixme: in future releases we may use type-changing struct update syntax like:
// Builder { inner: RustSBI { timer: None, ..self.inner } }
// https://github.com/rust-lang/rust/issues/86555

// fixme: struct `Infallible` should be replaced to never type once it's stablized

impl<T, I, R, H, S, P> Builder<T, I, R, H, S, P> {
    /// Add Timer programmer extension to RustSBI
    #[inline]
    pub fn with_timer<T2: Timer>(self, timer: T2) -> Builder<T2, I, R, H, S, P> {
        Builder {
            inner: RustSBI {
                timer: Some(timer),
                ipi: self.inner.ipi,
                rfnc: self.inner.rfnc,
                hsm: self.inner.hsm,
                srst: self.inner.srst,
                pmu: self.inner.pmu,
                #[cfg(not(feature = "machine"))]
                info: self.inner.info,
            },
        }
    }

    /// Add Inter-processor Interrupt extension to RustSBI
    #[inline]
    pub fn with_ipi<I2: Ipi>(self, ipi: I2) -> Builder<T, I2, R, H, S, P> {
        Builder {
            inner: RustSBI {
                timer: self.inner.timer,
                ipi: Some(ipi),
                rfnc: self.inner.rfnc,
                hsm: self.inner.hsm,
                srst: self.inner.srst,
                pmu: self.inner.pmu,
                #[cfg(not(feature = "machine"))]
                info: self.inner.info,
            },
        }
    }

    /// Add Remote Fence extension to RustSBI
    #[inline]
    pub fn with_fence<R2: Fence>(self, fence: R2) -> Builder<T, I, R2, H, S, P> {
        Builder {
            inner: RustSBI {
                timer: self.inner.timer,
                ipi: self.inner.ipi,
                rfnc: Some(fence),
                hsm: self.inner.hsm,
                srst: self.inner.srst,
                pmu: self.inner.pmu,
                #[cfg(not(feature = "machine"))]
                info: self.inner.info,
            },
        }
    }

    /// Add Hart State Monitor extension to RustSBI
    #[inline]
    pub fn with_hsm<H2: Hsm>(self, hsm: H2) -> Builder<T, I, R, H2, S, P> {
        Builder {
            inner: RustSBI {
                timer: self.inner.timer,
                ipi: self.inner.ipi,
                rfnc: self.inner.rfnc,
                hsm: Some(hsm),
                srst: self.inner.srst,
                pmu: self.inner.pmu,
                #[cfg(not(feature = "machine"))]
                info: self.inner.info,
            },
        }
    }

    /// Add System Reset extension to RustSBI
    #[inline]
    pub fn with_reset<S2: Reset>(self, reset: S2) -> Builder<T, I, R, H, S2, P> {
        Builder {
            inner: RustSBI {
                timer: self.inner.timer,
                ipi: self.inner.ipi,
                rfnc: self.inner.rfnc,
                hsm: self.inner.hsm,
                srst: Some(reset),
                pmu: self.inner.pmu,
                #[cfg(not(feature = "machine"))]
                info: self.inner.info,
            },
        }
    }

    /// Add Performance Monitor Unit extension to RustSBI
    #[inline]
    pub fn with_pmu<P2: Pmu>(self, pmu: P2) -> Builder<T, I, R, H, S, P2> {
        Builder {
            inner: RustSBI {
                timer: self.inner.timer,
                ipi: self.inner.ipi,
                rfnc: self.inner.rfnc,
                hsm: self.inner.hsm,
                srst: self.inner.srst,
                pmu: Some(pmu),
                #[cfg(not(feature = "machine"))]
                info: self.inner.info,
            },
        }
    }

    /// Build the target RustSBI instance
    #[inline]
    pub fn build(self) -> RustSBI<T, I, R, H, S, P> {
        self.inner
    }
}

// Placeholder for a structure that implements all RustSBI traits but is never accessed

// fixme: Should be replaced to never type `!` once it's stablized
// https://github.com/rust-lang/rust/issues/35121

// fixme: should be replaced to impl SomeTrait for ! once never type is stablized

impl Timer for Infallible {
    fn set_timer(&self, _: u64) {
        unreachable!()
    }
}

impl Ipi for Infallible {
    fn send_ipi(&self, _: HartMask) -> SbiRet {
        unreachable!()
    }
}

impl Fence for Infallible {
    fn remote_fence_i(&self, _: HartMask) -> SbiRet {
        unreachable!()
    }

    fn remote_sfence_vma(&self, _: HartMask, _: usize, _: usize) -> SbiRet {
        unreachable!()
    }

    fn remote_sfence_vma_asid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet {
        unreachable!()
    }

    fn remote_hfence_gvma_vmid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet {
        unreachable!()
    }

    fn remote_hfence_gvma(&self, _: HartMask, _: usize, _: usize) -> SbiRet {
        unreachable!()
    }

    fn remote_hfence_vvma_asid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet {
        unreachable!()
    }

    fn remote_hfence_vvma(&self, _: HartMask, _: usize, _: usize) -> SbiRet {
        unreachable!()
    }
}

impl Hsm for Infallible {
    fn hart_start(&self, _: usize, _: usize, _: usize) -> SbiRet {
        unreachable!()
    }

    fn hart_stop(&self) -> SbiRet {
        unreachable!()
    }

    fn hart_get_status(&self, _: usize) -> SbiRet {
        unreachable!()
    }

    fn hart_suspend(&self, _: u32, _: usize, _: usize) -> SbiRet {
        unreachable!()
    }
}

impl Reset for Infallible {
    fn system_reset(&self, _: u32, _: u32) -> SbiRet {
        unreachable!()
    }
    // no leagcy_reset here, instance based interface is not compatible to legacy extension
}

impl Pmu for Infallible {
    fn num_counters(&self) -> usize {
        unreachable!()
    }

    fn counter_get_info(&self, _: usize) -> SbiRet {
        unreachable!()
    }

    fn counter_config_matching(&self, _: usize, _: usize, _: usize, _: usize, _: u64) -> SbiRet {
        unreachable!()
    }

    fn counter_start(&self, _: usize, _: usize, _: usize, _: u64) -> SbiRet {
        unreachable!()
    }

    fn counter_stop(&self, _: usize, _: usize, _: usize) -> SbiRet {
        unreachable!()
    }

    fn counter_fw_read(&self, _: usize) -> SbiRet {
        unreachable!()
    }
}
