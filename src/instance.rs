use crate::{
    spec::binary::SbiRet, Console, Cppc, Fence, HartMask, Hsm, Ipi, Pmu, Reset, Susp, Timer,
    IMPL_ID_RUSTSBI, RUSTSBI_VERSION, SBI_SPEC_MAJOR, SBI_SPEC_MINOR,
};
use core::convert::Infallible;
#[cfg(feature = "machine")]
use riscv::register::{marchid, mimpid, mvendorid};
use spec::binary::Physical;

/// RustSBI instance including standard extensions
///
/// By now RustSBI supports to run instance based interface on systems has environment pointer width
/// that is the same as supervisor pointer width.
#[derive(Clone, Debug)]
pub struct RustSBI<T, I, R, H, S, P, C, SU, CP> {
    timer: Option<T>,
    ipi: Option<I>,
    rfnc: Option<R>,
    hsm: Option<H>,
    srst: Option<S>,
    pmu: Option<P>,
    dbcn: Option<C>,
    susp: Option<SU>,
    cppc: Option<CP>,
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

impl<T: Timer, I: Ipi, R: Fence, H: Hsm, S: Reset, P: Pmu, C: Console, SU: Susp, CP: Cppc>
    RustSBI<T, I, R, H, S, P, C, SU, CP>
{
    /// Create RustSBI instance on current machine environment for all the SBI extensions
    #[cfg(feature = "machine")]
    #[inline]
    pub const fn new_machine(
        timer: T,
        ipi: I,
        rfnc: R,
        hsm: H,
        srst: S,
        pmu: P,
        dbcn: C,
        susp: SU,
        cppc: CP,
    ) -> Self {
        Self {
            timer: Some(timer),
            ipi: Some(ipi),
            rfnc: Some(rfnc),
            hsm: Some(hsm),
            srst: Some(srst),
            pmu: Some(pmu),
            dbcn: Some(dbcn),
            susp: Some(susp),
            cppc: Some(cppc),
        }
    }

    /// Create RustSBI instance on given machine information for all the SBI extensions
    #[cfg(not(feature = "machine"))]
    #[allow(clippy::too_many_arguments)] // fixme: is it possible to have a better design here?
    #[inline]
    pub const fn with_machine_info(
        timer: T,
        ipi: I,
        rfnc: R,
        hsm: H,
        srst: S,
        pmu: P,
        dbcn: C,
        susp: SU,
        cppc: CP,
        info: MachineInfo,
    ) -> Self {
        Self {
            timer: Some(timer),
            ipi: Some(ipi),
            rfnc: Some(rfnc),
            hsm: Some(hsm),
            srst: Some(srst),
            pmu: Some(pmu),
            dbcn: Some(dbcn),
            susp: Some(susp),
            cppc: Some(cppc),
            info,
        }
    }
    /// Handle supervisor environment call with given parameters and return the `SbiRet` result.
    #[inline]
    pub fn handle_ecall(&mut self, extension: usize, function: usize, param: [usize; 6]) -> SbiRet {
        match extension {
            spec::rfnc::EID_RFNC => {
                let Some(rfnc) = &mut self.rfnc else {
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
                    let Some(timer) = &mut self.timer else {
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
                    let Some(timer) = &mut self.timer else {
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
                let Some(ipi) = &mut self.ipi else {
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
                let Some(hsm) = &mut self.hsm else {
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
                let Some(srst) = &mut self.srst else {
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
                    let Some(pmu) = &mut self.pmu else {
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
                        spec::pmu::PMU_COUNTER_FW_READ_HI => pmu.counter_fw_read(param0),
                        _ => SbiRet::not_supported(),
                    }
                }
                #[cfg(target_pointer_width = "32")]
                () => {
                    let Some(pmu) = &mut self.pmu else {
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
                        spec::pmu::PMU_COUNTER_FW_READ_HI => pmu.counter_fw_read(param0),
                        _ => SbiRet::not_supported(),
                    }
                }
            },
            spec::dbcn::EID_DBCN => {
                let Some(dbcn) = &mut self.dbcn else {
                    return SbiRet::not_supported();
                };
                let [param0, param1, param2] = [param[0], param[1], param[2]];
                match function {
                    spec::dbcn::CONSOLE_WRITE => {
                        let bytes = Physical::new(param0, param1, param2);
                        dbcn.write(bytes)
                    }
                    spec::dbcn::CONSOLE_READ => {
                        let bytes = Physical::new(param0, param1, param2);
                        dbcn.read(bytes)
                    }
                    spec::dbcn::CONSOLE_WRITE_BYTE => dbcn.write_byte((param0 & 0xFF) as u8),
                    _ => SbiRet::not_supported(),
                }
            }
            spec::susp::EID_SUSP => {
                let Some(susp) = &mut self.susp else {
                    return SbiRet::not_supported();
                };
                let [param0, param1, param2] = [param[0], param[1], param[2]];
                match function {
                    spec::susp::SUSPEND => match u32::try_from(param0) {
                        Ok(sleep_type) => susp.system_suspend(sleep_type, param1, param2),
                        _ => SbiRet::invalid_param(),
                    },
                    _ => SbiRet::not_supported(),
                }
            }
            spec::cppc::EID_CPPC => match () {
                #[cfg(target_pointer_width = "64")]
                () => {
                    let Some(cppc) = &mut self.cppc else {
                        return SbiRet::not_supported();
                    };
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
                    let Some(cppc) = &mut self.cppc else {
                        return SbiRet::not_supported();
                    };
                    let [param0, param1, param2] = [param[0], param[1], param[2]];
                    match function {
                        spec::cppc::PROBE => cppc.probe(param0 as _),
                        spec::cppc::READ => cppc.read(param0 as _),
                        spec::cppc::READ_HI => cppc.read_hi(param0 as _),
                        spec::cppc::WRITE => cppc.write(param0 as _, concat_u32(param2, param1)),
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
            spec::dbcn::EID_DBCN => self.dbcn.is_some(),
            spec::susp::EID_SUSP => self.susp.is_some(),
            spec::cppc::EID_CPPC => self.cppc.is_some(),
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
pub struct Builder<T, I, R, H, S, P, C, SU, CP> {
    inner: RustSBI<T, I, R, H, S, P, C, SU, CP>,
}

impl
    Builder<
        Infallible,
        Infallible,
        Infallible,
        Infallible,
        Infallible,
        Infallible,
        Infallible,
        Infallible,
        Infallible,
    >
{
    /// Create a new `Builder` from current machine environment
    #[inline]
    #[cfg(feature = "machine")]
    pub const fn new_machine() -> Builder<
        Infallible,
        Infallible,
        Infallible,
        Infallible,
        Infallible,
        Infallible,
        Infallible,
        Infallible,
        Infallible,
    > {
        Builder {
            inner: RustSBI {
                timer: None,
                ipi: None,
                rfnc: None,
                hsm: None,
                srst: None,
                pmu: None,
                dbcn: None,
                susp: None,
                cppc: None,
            },
        }
    }

    /// Create a new `Builder` from machine information
    #[inline]
    #[cfg(not(feature = "machine"))]
    pub const fn with_machine_info(
        info: MachineInfo,
    ) -> Builder<
        Infallible,
        Infallible,
        Infallible,
        Infallible,
        Infallible,
        Infallible,
        Infallible,
        Infallible,
        Infallible,
    > {
        Builder {
            inner: RustSBI {
                timer: None,
                ipi: None,
                rfnc: None,
                hsm: None,
                srst: None,
                pmu: None,
                dbcn: None,
                susp: None,
                cppc: None,
                info,
            },
        }
    }
}

// fixme: in future releases we may use type-changing struct update syntax like:
// Builder { inner: RustSBI { timer: None, ..self.inner } }
// https://github.com/rust-lang/rust/issues/86555

// fixme: struct `Infallible` should be replaced to never type once it's stablized

impl<T, I, R, H, S, P, C, SU, CP> Builder<T, I, R, H, S, P, C, SU, CP> {
    /// Add Timer programmer extension to RustSBI
    #[inline]
    pub fn with_timer<T2: Timer>(self, timer: T2) -> Builder<T2, I, R, H, S, P, C, SU, CP> {
        Builder {
            inner: RustSBI {
                timer: Some(timer),
                ipi: self.inner.ipi,
                rfnc: self.inner.rfnc,
                hsm: self.inner.hsm,
                srst: self.inner.srst,
                pmu: self.inner.pmu,
                dbcn: self.inner.dbcn,
                susp: self.inner.susp,
                cppc: self.inner.cppc,
                #[cfg(not(feature = "machine"))]
                info: self.inner.info,
            },
        }
    }

    /// Add Inter-processor Interrupt extension to RustSBI
    #[inline]
    pub fn with_ipi<I2: Ipi>(self, ipi: I2) -> Builder<T, I2, R, H, S, P, C, SU, CP> {
        Builder {
            inner: RustSBI {
                timer: self.inner.timer,
                ipi: Some(ipi),
                rfnc: self.inner.rfnc,
                hsm: self.inner.hsm,
                srst: self.inner.srst,
                pmu: self.inner.pmu,
                dbcn: self.inner.dbcn,
                susp: self.inner.susp,
                cppc: self.inner.cppc,
                #[cfg(not(feature = "machine"))]
                info: self.inner.info,
            },
        }
    }

    /// Add Remote Fence extension to RustSBI
    #[inline]
    pub fn with_fence<R2: Fence>(self, fence: R2) -> Builder<T, I, R2, H, S, P, C, SU, CP> {
        Builder {
            inner: RustSBI {
                timer: self.inner.timer,
                ipi: self.inner.ipi,
                rfnc: Some(fence),
                hsm: self.inner.hsm,
                srst: self.inner.srst,
                pmu: self.inner.pmu,
                dbcn: self.inner.dbcn,
                susp: self.inner.susp,
                cppc: self.inner.cppc,
                #[cfg(not(feature = "machine"))]
                info: self.inner.info,
            },
        }
    }

    /// Add Hart State Monitor extension to RustSBI
    #[inline]
    pub fn with_hsm<H2: Hsm>(self, hsm: H2) -> Builder<T, I, R, H2, S, P, C, SU, CP> {
        Builder {
            inner: RustSBI {
                timer: self.inner.timer,
                ipi: self.inner.ipi,
                rfnc: self.inner.rfnc,
                hsm: Some(hsm),
                srst: self.inner.srst,
                pmu: self.inner.pmu,
                dbcn: self.inner.dbcn,
                susp: self.inner.susp,
                cppc: self.inner.cppc,
                #[cfg(not(feature = "machine"))]
                info: self.inner.info,
            },
        }
    }

    /// Add System Reset extension to RustSBI
    #[inline]
    pub fn with_reset<S2: Reset>(self, reset: S2) -> Builder<T, I, R, H, S2, P, C, SU, CP> {
        Builder {
            inner: RustSBI {
                timer: self.inner.timer,
                ipi: self.inner.ipi,
                rfnc: self.inner.rfnc,
                hsm: self.inner.hsm,
                srst: Some(reset),
                pmu: self.inner.pmu,
                dbcn: self.inner.dbcn,
                susp: self.inner.susp,
                cppc: self.inner.cppc,
                #[cfg(not(feature = "machine"))]
                info: self.inner.info,
            },
        }
    }

    /// Add Performance Monitor Unit extension to RustSBI
    #[inline]
    pub fn with_pmu<P2: Pmu>(self, pmu: P2) -> Builder<T, I, R, H, S, P2, C, SU, CP> {
        Builder {
            inner: RustSBI {
                timer: self.inner.timer,
                ipi: self.inner.ipi,
                rfnc: self.inner.rfnc,
                hsm: self.inner.hsm,
                srst: self.inner.srst,
                pmu: Some(pmu),
                dbcn: self.inner.dbcn,
                susp: self.inner.susp,
                cppc: self.inner.cppc,
                #[cfg(not(feature = "machine"))]
                info: self.inner.info,
            },
        }
    }
    /// Add Debug Console extension to RustSBI
    #[inline]
    pub fn with_console<C2: Console>(self, console: C2) -> Builder<T, I, R, H, S, P, C2, SU, CP> {
        Builder {
            inner: RustSBI {
                timer: self.inner.timer,
                ipi: self.inner.ipi,
                rfnc: self.inner.rfnc,
                hsm: self.inner.hsm,
                srst: self.inner.srst,
                pmu: self.inner.pmu,
                dbcn: Some(console),
                susp: self.inner.susp,
                cppc: self.inner.cppc,
                #[cfg(not(feature = "machine"))]
                info: self.inner.info,
            },
        }
    }
    /// Add System Suspend extension to RustSBI
    #[inline]
    pub fn with_susp<SU2: Susp>(self, susp: SU2) -> Builder<T, I, R, H, S, P, C, SU2, CP> {
        Builder {
            inner: RustSBI {
                timer: self.inner.timer,
                ipi: self.inner.ipi,
                rfnc: self.inner.rfnc,
                hsm: self.inner.hsm,
                srst: self.inner.srst,
                pmu: self.inner.pmu,
                dbcn: self.inner.dbcn,
                susp: Some(susp),
                cppc: self.inner.cppc,
                #[cfg(not(feature = "machine"))]
                info: self.inner.info,
            },
        }
    }
    /// Add CPPC extension to RustSBI
    #[inline]
    pub fn with_cppc<CP2: Cppc>(self, cppc: CP2) -> Builder<T, I, R, H, S, P, C, SU, CP2> {
        Builder {
            inner: RustSBI {
                timer: self.inner.timer,
                ipi: self.inner.ipi,
                rfnc: self.inner.rfnc,
                hsm: self.inner.hsm,
                srst: self.inner.srst,
                pmu: self.inner.pmu,
                dbcn: self.inner.dbcn,
                susp: self.inner.susp,
                cppc: Some(cppc),
                #[cfg(not(feature = "machine"))]
                info: self.inner.info,
            },
        }
    }

    /// Build the target RustSBI instance
    #[inline]
    pub fn build(self) -> RustSBI<T, I, R, H, S, P, C, SU, CP> {
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

    fn counter_fw_read_hi(&self, _: usize) -> SbiRet {
        unreachable!()
    }
}

impl Console for Infallible {
    fn write(&self, _: Physical<&[u8]>) -> SbiRet {
        unreachable!()
    }

    fn read(&self, _: Physical<&mut [u8]>) -> SbiRet {
        unreachable!()
    }

    fn write_byte(&self, _: u8) -> SbiRet {
        unreachable!()
    }
}

impl Susp for Infallible {
    fn system_suspend(&self, _: u32, _: usize, _: usize) -> SbiRet {
        unreachable!()
    }
}

impl Cppc for Infallible {
    fn probe(&self, _: u32) -> SbiRet {
        unreachable!()
    }
    fn read(&self, _: u32) -> SbiRet {
        unreachable!()
    }
    fn read_hi(&self, _: u32) -> SbiRet {
        unreachable!()
    }
    fn write(&self, _: u32, _: u64) -> SbiRet {
        unreachable!()
    }
}
