use spec::{
    binary::{SbiRet, SharedPtr},
    nacl::shmem_size::NATIVE,
};

/// Nested Acceleration Extension
///
/// Nested virtualization is the ability of a hypervisor to run another hypervisor
/// as a guest. RISC-V nested virtualization requires an L0 hypervisor (running
/// in hypervisor-mode) to trap-and-emulate the RISC-V H-extension functionality
/// (such as CSR accesses, HFENCE instructions, HLV/HSV instructions,
/// etc.) for the L1 hypervisor (running in virtualized supervisor-mode).
///
/// The SBI nested acceleration extension defines a shared memory based interface
/// between the SBI implementation (or L0 hypervisor) and the supervisor software
/// (or L1 hypervisor) which allows both to collaboratively reduce traps taken
/// by the L0 hypervisor for emulating RISC-V H-extension functionality. The
/// nested acceleration shared memory allows the L1 hypervisor to batch multiple
/// RISC-V H-extension CSR accesses and HFENCE requests which are then emulated
/// by the L0 hypervisor upon an explicit synchronization SBI call.
///
/// *NOTE:* The M-mode firmware should not implement the SBI nested acceleration
/// extension if the underlying platform has the RISC-V H-extension implemented
/// in hardware.

pub trait Nacl: Send + Sync {
    /// Probe nested acceleration feature.
    ///
    /// Probe a nested acceleration feature. This is a mandatory function of the
    /// SBI nested acceleration extension.
    ///
    /// # Parameters
    ///
    /// The `feature_id` parameter specifies the nested acceleration feature to probe.
    ///
    /// # Return Value
    ///
    /// This function always returns SBI_SUCCESS in `SbiRet.error`. It returns `0`
    /// in `SbiRet.value` if the given `feature_id` is not available, or `1` in
    /// `SbiRet.value` if it is available.
    fn probe_feature(&self, feature_id: u32) -> SbiRet;
    /// Set nested acceleration shared memory.
    ///
    /// Set and enable the shared memory for nested acceleration on the calling
    /// hart.
    ///
    /// # Parameters
    ///
    /// If physical address of `shmem` are not all-ones bitwise, then the `shmem` pointer
    /// specifies the shared memory physical base address. The physical address of `shmem`
    /// MUST be 4096-byte aligned. The size of the shared memory must be 4096 + (XLEN * 128) bytes.
    /// The SBI implementation MUST zero the shared memory before returning from the SBI
    /// call.
    ///
    /// If physical address of `shmem` are all-ones bitwise, then the nested acceleration features
    /// are disabled.
    ///
    /// The `flags` parameter is reserved for future use and must be zero.
    ///
    /// # Return Value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Return code                 | Description
    /// |:----------------------------|:----------------------------------------------
    /// | `SbiRet::success()`         | The steal-time shared memory physical base address was set or cleared successfully.
    /// | `SbiRet::invalid_param()`   | The `flags` parameter is not zero or the `shmem` is not 4096-byte aligned.
    /// | `SbiRet::invalid_address()` | The shared memory pointed to by the `shmem` parameter is not writable or does not satisfy other requirements of shared memory physical address range.
    fn set_shmem(&self, shmem: SharedPtr<[u8; NATIVE]>, flags: usize) -> SbiRet;
    /// Synchronize shared memory CSRs.
    ///
    /// Synchronize CSRs in the nested acceleration shared memory. This is an
    /// optional function which is only available if the SBI_NACL_FEAT_SYNC_CSR
    /// feature is available.
    ///
    /// # Parameters
    ///
    /// The parameter `csr_num` specifies the set of RISC-V H-extension CSRs to be synchronized.
    ///
    /// If `csr_num` is all-ones bitwise then all RISC-V H-extension CSRs implemented by the SBI
    /// implementation (or L0 hypervisor) are synchronized.
    ///
    /// If `(csr_num & 0x300) == 0x200` and `csr_num < 0x1000` then only a single RISC-V H-extension
    /// CSR specified by the `csr_num` parameter is synchronized.
    ///
    /// # Return Value
    ///
    /// The possible error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Return code               | Description
    /// |:--------------------------|:----------------------------------------------
    /// | `SbiRet::success()`       | CSRs synchronized successfully.
    /// | `SbiRet::not_supported()` | SBI_NACL_FEAT_SYNC_CSR feature is not available.
    /// | `SbiRet::invalid_param()` | `csr_num` is not all-ones bitwise and either: `(csr_num & 0x300) != 0x200` or `csr_num >= 0x1000` or `csr_num` is not implemented by the SBI implementation
    /// | `SbiRet::no_shmem()`      | Nested acceleration shared memory not available.
    fn sync_csr(&self, csr_num: usize) -> SbiRet;
    /// Synchronize shared memory HFENCEs.
    ///
    /// Synchronize HFENCEs in the nested acceleration shared memory. This is an
    /// optional function which is only available if the SBI_NACL_FEAT_SYNC_HFENCE
    /// feature is available.
    ///
    /// # Parameters
    ///
    /// The parameter `entry_index` specifies the set of nested HFENCE entries to be synchronized.
    ///
    /// If `entry_index` is all-ones bitwise then all nested HFENCE entries are
    /// synchronized.
    ///
    /// If `entry_index < (3840 / XLEN)` then only a single nested HFENCE entry
    /// specified by the `entry_index` parameter is synchronized.
    ///
    /// # Return Value
    ///
    /// The possible error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Return code               | Description
    /// |:--------------------------|:----------------------------------------------
    /// | `SbiRet::success()`       | HFENCEs synchronized successfully.
    /// | `SbiRet::not_supported()` | SBI_NACL_FEAT_SYNC_HFENCE feature is not available.
    /// | `SbiRet::invalid_param()` | `entry_index` is not all-ones bitwise and `entry_index >= (3840 / XLEN)`.
    /// | `SbiRet::no_shmem()`      | Nested acceleration shared memory not available.
    fn sync_hfence(&self, entry_index: usize) -> SbiRet;
    /// Synchronize shared memory and emulate SRET.
    ///
    /// Synchronize CSRs and HFENCEs in the nested acceleration shared memory and
    /// emulate the SRET instruction. This is an optional function which is only
    /// available if the SBI_NACL_FEAT_SYNC_SRET feature is available.
    ///
    /// This function is used by supervisor software (or L1 hypervisor) to do
    /// a synchronize SRET request and the SBI implementation (or L0 hypervisor)
    /// MUST handle it.
    ///
    /// # Return Value
    ///
    /// This function does not return upon success and the possible error codes returned in
    /// `SbiRet.error` upon failure are shown in the table below:
    ///
    /// | Return code               | Description
    /// |:--------------------------|:----------------------------------------------
    /// | `SbiRet::not_supported()` | SBI_NACL_FEAT_SYNC_SRET feature is not available.
    /// | `SbiRet::no_shmem()`      | Nested acceleration shared memory not available.
    fn sync_sret(&self) -> SbiRet;
}

impl<T: Nacl> Nacl for &T {
    #[inline]
    fn probe_feature(&self, feature_id: u32) -> SbiRet {
        T::probe_feature(self, feature_id)
    }
    #[inline]
    fn set_shmem(&self, shmem: SharedPtr<[u8; NATIVE]>, flags: usize) -> SbiRet {
        T::set_shmem(self, shmem, flags)
    }
    #[inline]
    fn sync_csr(&self, csr_num: usize) -> SbiRet {
        T::sync_csr(self, csr_num)
    }
    #[inline]
    fn sync_hfence(&self, entry_index: usize) -> SbiRet {
        T::sync_hfence(self, entry_index)
    }
    #[inline]
    fn sync_sret(&self) -> SbiRet {
        T::sync_sret(self)
    }
}
