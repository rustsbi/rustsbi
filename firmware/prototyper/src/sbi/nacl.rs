use rustsbi::SbiRet;
use sbi_spec::binary::SharedPtr;
use sbi_spec::nacl::shmem_size::NATIVE;

/// SBI Nested Acceleration extension implementation.
///
/// The target implements the RISC-V H-extension in hardware, so this
/// firmware provides no NACL features. The handlers remain available for
/// platforms that need to emulate H-extension operations.
pub(crate) struct SbiNacl;

impl rustsbi::Nacl for SbiNacl {
    fn probe_feature(&self, _feature_id: u32) -> SbiRet {
        // H is implemented in hardware, so no NACL feature is provided.
        SbiRet::success(0)
    }

    fn set_shmem(&self, shmem: SharedPtr<[u8; NATIVE]>, flags: usize) -> SbiRet {
        // The flags field is reserved and must be zero.
        if flags != 0 {
            return SbiRet::invalid_param();
        }

        let lo = shmem.phys_addr_lo();
        let hi = shmem.phys_addr_hi();

        // An all-ones pointer disables the NACL features.
        if hi == usize::MAX && lo == usize::MAX {
            return SbiRet::success(0);
        }

        // NACL shared memory must be page-aligned and addressable by usize.
        if lo & 0xfff != 0 || hi != 0 {
            return SbiRet::invalid_param();
        }

        // The supervisor must be able to write the entire shared-memory area.
        if !crate::firmware::supervisor_writable(lo, NATIVE) {
            return SbiRet::invalid_address();
        }

        // Safety: the range was validated as supervisor-writable above.
        unsafe {
            core::ptr::write_bytes(lo as *mut u8, 0, NATIVE);
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        }

        SbiRet::success(0)
    }

    fn sync_csr(&self, _csr_num: usize) -> SbiRet {
        SbiRet::not_supported()
    }

    fn sync_hfence(&self, _entry_index: usize) -> SbiRet {
        SbiRet::not_supported()
    }

    fn sync_sret(&self) -> SbiRet {
        SbiRet::not_supported()
    }

    fn _rustsbi_probe(&self) -> usize {
        sbi_spec::base::UNAVAILABLE_EXTENSION
    }
}
