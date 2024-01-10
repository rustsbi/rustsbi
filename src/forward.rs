use crate::{Console, Cppc, EnvInfo, Fence, Hsm, Ipi, Nacl, Pmu, Reset, Sta, Susp, Timer};
use sbi_spec::{
    binary::{HartMask, Physical, SbiRet, SharedPtr},
    nacl::shmem_size::NATIVE,
};

/// Forwards SBI calls onto current supervisor environment.
///
/// If crate feature `forward` is enabled, this structure implements all RustSBI extensions
/// by forwarding the calls into current supervisor environment. This is done by `sbi-rt`
/// crate; thus `Forward` is only available when it's running in RISC-V SBI environments.
///
/// `Forward` implements all RustSBI traits, but is only effective if `#[cfg(feature = "forward")]`
/// is enabled. Otherwise, struct `Forward` is `unimplemented!()` on SBI calls.
///
/// # Examples
///
/// This structure can be used as a structure field in `#[derive(RustSBI)]`, with helper
/// macro `#[rustsbi(extension_1, extension_2, ...)]` annotating what extensions should be
/// forwarded to `sbi-rt` in the structure.
///
/// ```rust
/// use rustsbi::{Forward, RustSBI};
///
/// // Forwards fence, timer and console extensions, but the hsm extension
/// // is still handled by the `hsm` field variable.
/// #[derive(RustSBI)]
/// struct VmSBI {
///     hsm: VmHsm,
///     #[rustsbi(fence, timer, console, info)]
///     forward: Forward,
/// }
///
/// # use sbi_spec::binary::SbiRet;
/// # struct VmHsm;
/// # impl rustsbi::Hsm for VmHsm {
/// #     fn hart_start(&self, _: usize, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// #     fn hart_stop(&self) -> SbiRet { unimplemented!() }
/// #     fn hart_get_status(&self, _: usize) -> SbiRet { unimplemented!() }
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Forward;

impl Console for Forward {
    #[inline]
    fn write(&self, bytes: Physical<&[u8]>) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::console_write(bytes),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = bytes;
                unimplemented!()
            }
        }
    }

    #[inline]
    fn read(&self, bytes: Physical<&mut [u8]>) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::console_read(bytes),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = bytes;
                unimplemented!()
            }
        }
    }

    #[inline]
    fn write_byte(&self, byte: u8) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::console_write_byte(byte),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = byte;
                unimplemented!()
            }
        }
    }
}

impl Cppc for Forward {
    #[inline]
    fn probe(&self, reg_id: u32) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::cppc_probe(reg_id),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = reg_id;
                unimplemented!()
            }
        }
    }

    #[inline]
    fn read(&self, reg_id: u32) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::cppc_read(reg_id),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = reg_id;
                unimplemented!()
            }
        }
    }

    #[inline]
    fn read_hi(&self, reg_id: u32) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::cppc_read_hi(reg_id),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = reg_id;
                unimplemented!()
            }
        }
    }

    #[inline]
    fn write(&self, reg_id: u32, val: u64) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::cppc_write(reg_id, val),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = (reg_id, val);
                unimplemented!()
            }
        }
    }
}

impl Fence for Forward {
    #[inline]
    fn remote_fence_i(&self, hart_mask: HartMask) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::remote_fence_i(hart_mask),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = hart_mask;
                unimplemented!()
            }
        }
    }

    #[inline]
    fn remote_sfence_vma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::remote_sfence_vma(hart_mask, start_addr, size),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = (hart_mask, start_addr, size);
                unimplemented!()
            }
        }
    }

    #[inline]
    fn remote_sfence_vma_asid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::remote_sfence_vma_asid(hart_mask, start_addr, size, asid),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = (hart_mask, start_addr, size, asid);
                unimplemented!()
            }
        }
    }

    #[inline]
    fn remote_hfence_gvma_vmid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        vmid: usize,
    ) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::remote_hfence_gvma_vmid(hart_mask, start_addr, size, vmid),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = (hart_mask, start_addr, size, vmid);
                unimplemented!()
            }
        }
    }

    #[inline]
    fn remote_hfence_gvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::remote_hfence_gvma(hart_mask, start_addr, size),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = (hart_mask, start_addr, size);
                unimplemented!()
            }
        }
    }

    #[inline]
    fn remote_hfence_vvma_asid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::remote_hfence_vvma_asid(hart_mask, start_addr, size, asid),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = (hart_mask, start_addr, size, asid);
                unimplemented!()
            }
        }
    }

    #[inline]
    fn remote_hfence_vvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::remote_hfence_vvma(hart_mask, start_addr, size),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = (hart_mask, start_addr, size);
                unimplemented!()
            }
        }
    }
}

impl Hsm for Forward {
    #[inline]
    fn hart_start(&self, hartid: usize, start_addr: usize, opaque: usize) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::hart_start(hartid, start_addr, opaque),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = (hartid, start_addr, opaque);
                unimplemented!()
            }
        }
    }

    #[inline]
    fn hart_stop(&self) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::hart_stop(),
            #[cfg(not(feature = "forward"))]
            () => {
                unimplemented!()
            }
        }
    }

    #[inline]
    fn hart_get_status(&self, hartid: usize) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::hart_get_status(hartid),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = hartid;
                unimplemented!()
            }
        }
    }

    #[inline]
    fn hart_suspend(&self, suspend_type: u32, resume_addr: usize, opaque: usize) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::hart_suspend(suspend_type, resume_addr, opaque),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = (suspend_type, resume_addr, opaque);
                unimplemented!()
            }
        }
    }
}

impl Ipi for Forward {
    #[inline]
    fn send_ipi(&self, hart_mask: HartMask) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::send_ipi(hart_mask),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = hart_mask;
                unimplemented!()
            }
        }
    }
}

impl Nacl for Forward {
    #[inline]
    fn probe_feature(&self, feature_id: u32) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::nacl_probe_feature(feature_id),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = feature_id;
                unimplemented!()
            }
        }
    }

    #[inline]
    fn set_shmem(&self, shmem: SharedPtr<[u8; NATIVE]>, flags: usize) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::nacl_set_shmem(shmem, flags),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = (shmem, flags);
                unimplemented!()
            }
        }
    }

    #[inline]
    fn sync_csr(&self, csr_num: usize) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::nacl_sync_csr(csr_num),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = csr_num;
                unimplemented!()
            }
        }
    }

    #[inline]
    fn sync_hfence(&self, entry_index: usize) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::nacl_sync_hfence(entry_index),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = entry_index;
                unimplemented!()
            }
        }
    }

    #[inline]
    fn sync_sret(&self) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::nacl_sync_sret(),
            #[cfg(not(feature = "forward"))]
            () => unimplemented!(),
        }
    }
}

impl Pmu for Forward {
    #[inline]
    fn num_counters(&self) -> usize {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::pmu_num_counters(),
            #[cfg(not(feature = "forward"))]
            () => unimplemented!(),
        }
    }

    #[inline]
    fn counter_get_info(&self, counter_idx: usize) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::pmu_counter_get_info(counter_idx),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = counter_idx;
                unimplemented!()
            }
        }
    }

    #[inline]
    fn counter_config_matching(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        config_flags: usize,
        event_idx: usize,
        event_data: u64,
    ) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::pmu_counter_config_matching(
                counter_idx_base,
                counter_idx_mask,
                config_flags,
                event_idx,
                event_data,
            ),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = (
                    counter_idx_base,
                    counter_idx_mask,
                    config_flags,
                    event_idx,
                    event_data,
                );
                unimplemented!()
            }
        }
    }

    #[inline]
    fn counter_start(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        start_flags: usize,
        initial_value: u64,
    ) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::pmu_counter_start(
                counter_idx_base,
                counter_idx_mask,
                start_flags,
                initial_value,
            ),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = (
                    counter_idx_base,
                    counter_idx_mask,
                    start_flags,
                    initial_value,
                );
                unimplemented!()
            }
        }
    }

    #[inline]
    fn counter_stop(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        stop_flags: usize,
    ) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::pmu_counter_stop(counter_idx_base, counter_idx_mask, stop_flags),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = (counter_idx_base, counter_idx_mask, stop_flags);
                unimplemented!()
            }
        }
    }

    #[inline]
    fn counter_fw_read(&self, counter_idx: usize) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::pmu_counter_fw_read(counter_idx),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = counter_idx;
                unimplemented!()
            }
        }
    }

    #[inline]
    fn counter_fw_read_hi(&self, counter_idx: usize) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::pmu_counter_fw_read_hi(counter_idx),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = counter_idx;
                unimplemented!()
            }
        }
    }
}

impl Reset for Forward {
    #[inline]
    fn system_reset(&self, reset_type: u32, reset_reason: u32) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::system_reset(reset_type, reset_reason),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = (reset_type, reset_reason);
                unimplemented!()
            }
        }
    }
}

impl Sta for Forward {
    #[inline]
    fn set_shmem(&self, shmem: SharedPtr<[u8; 64]>, flags: usize) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::sta_set_shmem(shmem, flags),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = (shmem, flags);
                unimplemented!()
            }
        }
    }
}

impl Susp for Forward {
    #[inline]
    fn system_suspend(&self, sleep_type: u32, resume_addr: usize, opaque: usize) -> SbiRet {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::system_suspend(sleep_type, resume_addr, opaque),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = (sleep_type, resume_addr, opaque);
                unimplemented!()
            }
        }
    }
}

impl Timer for Forward {
    #[inline]
    fn set_timer(&self, stime_value: u64) {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::set_timer(stime_value),
            #[cfg(not(feature = "forward"))]
            () => {
                let _ = stime_value;
                unimplemented!()
            }
        };
    }
}

impl EnvInfo for Forward {
    #[inline]
    fn mvendorid(&self) -> usize {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::get_mvendorid(),
            #[cfg(not(feature = "forward"))]
            () => unimplemented!(),
        }
    }

    #[inline]
    fn marchid(&self) -> usize {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::get_marchid(),
            #[cfg(not(feature = "forward"))]
            () => unimplemented!(),
        }
    }

    #[inline]
    fn mimpid(&self) -> usize {
        match () {
            #[cfg(feature = "forward")]
            () => sbi_rt::get_mimpid(),
            #[cfg(not(feature = "forward"))]
            () => unimplemented!(),
        }
    }
}
