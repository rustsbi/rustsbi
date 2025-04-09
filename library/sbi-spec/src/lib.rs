//! RISC-V SBI Specification structure and constant definitions.
//!
//! This crate adapts to RISC-V SBI Specification version 2.0 ratified.
//! It provides structures in Rust semantics and best practices to simplify
//! designs of RISC-V SBI ecosystem, both implementation and applications.
//!
//! You may find it convenient to use this library in a vast range of packages,
//! from operating system kernels, hypervisors, to SBI bare metal implementations.
//! This crate is `no_std` compatible and does not need dynamic memory allocation,
//! which makes it suitable for embedded development.
//!
//! Although this library is dedicated to RISC-V architecture, it does not limit
//! which build target the dependents should compile into.
//! For example, when developing a RISC-V emulator on platforms other than RISC-V,
//! the emulator designed on other platforms can still make use of `sbi-spec` structures,
//! to provide the necessary features where the emulated RISC-V environment would make use of.
#![no_std]
#![deny(missing_docs, unstable_features)]

// §3
pub mod binary;
// §4
pub mod base;
// §5
#[cfg(feature = "legacy")]
pub mod legacy;
// §6
pub mod time;
// §7
pub mod spi;
// §8
pub mod rfnc;
// §9
pub mod hsm;
// §10
pub mod srst;
// §11
pub mod pmu;
// §12
pub mod dbcn;
// §13
pub mod susp;
// §14
pub mod cppc;
// §15
pub mod nacl;
// §16
pub mod sta;
// §17
pub mod sse;
// §18
pub mod fwft;
// §19
pub mod dbtr;
// §20
pub mod mpxy;

/// Converts SBI EID from str.
const fn eid_from_str(name: &str) -> i32 {
    match *name.as_bytes() {
        [a] => i32::from_be_bytes([0, 0, 0, a]),
        [a, b] => i32::from_be_bytes([0, 0, a, b]),
        [a, b, c] => i32::from_be_bytes([0, a, b, c]),
        [a, b, c, d] => i32::from_be_bytes([a, b, c, d]),
        _ => unreachable!(),
    }
}

/// Checks during compilation, and provides an item list for developers.
#[cfg(test)]
mod tests {
    use static_assertions::{
        assert_eq_align, assert_eq_size, assert_fields, assert_impl_all, const_assert_eq,
    };
    // §3
    #[test]
    fn test_binary() {
        use crate::binary::*;
        assert_eq_align!(SbiRet, usize);
        assert_eq_size!(SbiRet, [usize; 2]);
        assert_fields!(SbiRet<usize>: error);
        assert_fields!(SbiRet<usize>: value);
        assert_impl_all!(SbiRet: Copy, Clone, PartialEq, Eq, core::fmt::Debug);

        const_assert_eq!(0, RET_SUCCESS as isize);
        const_assert_eq!(-1, RET_ERR_FAILED as isize);
        const_assert_eq!(-2, RET_ERR_NOT_SUPPORTED as isize);
        const_assert_eq!(-3, RET_ERR_INVALID_PARAM as isize);
        const_assert_eq!(-4, RET_ERR_DENIED as isize);
        const_assert_eq!(-5, RET_ERR_INVALID_ADDRESS as isize);
        const_assert_eq!(-6, RET_ERR_ALREADY_AVAILABLE as isize);
        const_assert_eq!(-7, RET_ERR_ALREADY_STARTED as isize);
        const_assert_eq!(-8, RET_ERR_ALREADY_STOPPED as isize);
        const_assert_eq!(-9, RET_ERR_NO_SHMEM as isize);
        const_assert_eq!(-10, RET_ERR_INVALID_STATE as isize);
        const_assert_eq!(-11, RET_ERR_BAD_RANGE as isize);
        const_assert_eq!(-12, RET_ERR_TIMEOUT as isize);
        const_assert_eq!(-13, RET_ERR_IO as isize);
        const_assert_eq!(-14, RET_ERR_DENIED_LOCKED as isize);
    }
    // §4
    #[test]
    fn test_base() {
        use crate::base::*;
        const_assert_eq!(0x10, EID_BASE);
        const_assert_eq!(0, GET_SBI_SPEC_VERSION);
        const_assert_eq!(1, GET_SBI_IMPL_ID);
        const_assert_eq!(2, GET_SBI_IMPL_VERSION);
        const_assert_eq!(3, PROBE_EXTENSION);
        const_assert_eq!(4, GET_MVENDORID);
        const_assert_eq!(5, GET_MARCHID);
        const_assert_eq!(6, GET_MIMPID);
        const_assert_eq!(0, impl_id::BBL);
        const_assert_eq!(1, impl_id::OPEN_SBI);
        const_assert_eq!(2, impl_id::XVISOR);
        const_assert_eq!(3, impl_id::KVM);
        const_assert_eq!(4, impl_id::RUST_SBI);
        const_assert_eq!(5, impl_id::DIOSIX);
        const_assert_eq!(6, impl_id::COFFER);
        const_assert_eq!(7, impl_id::XEN);
        const_assert_eq!(8, impl_id::POLARFIRE_HSS);
        const_assert_eq!(9, impl_id::COREBOOT);
        const_assert_eq!(10, impl_id::OREBOOT);
    }
    // §5
    #[cfg(feature = "legacy")]
    #[test]
    fn test_legacy() {
        use crate::legacy::*;
        const_assert_eq!(0, LEGACY_SET_TIMER);
        const_assert_eq!(1, LEGACY_CONSOLE_PUTCHAR);
        const_assert_eq!(2, LEGACY_CONSOLE_GETCHAR);
        const_assert_eq!(3, LEGACY_CLEAR_IPI);
        const_assert_eq!(4, LEGACY_SEND_IPI);
        const_assert_eq!(5, LEGACY_REMOTE_FENCE_I);
        const_assert_eq!(6, LEGACY_REMOTE_SFENCE_VMA);
        const_assert_eq!(7, LEGACY_REMOTE_SFENCE_VMA_ASID);
        const_assert_eq!(8, LEGACY_SHUTDOWN);
    }
    // §6
    #[test]
    fn test_time() {
        use crate::time::*;
        const_assert_eq!(0x54494D45, EID_TIME);
        const_assert_eq!(0, SET_TIMER);
    }
    // §7
    #[test]
    fn test_spi() {
        use crate::spi::*;
        const_assert_eq!(0x735049, EID_SPI);
        const_assert_eq!(0, SEND_IPI);
    }
    // §8
    #[test]
    fn test_rfnc() {
        use crate::rfnc::*;
        const_assert_eq!(0x52464E43, EID_RFNC);
        const_assert_eq!(0, REMOTE_FENCE_I);
        const_assert_eq!(1, REMOTE_SFENCE_VMA);
        const_assert_eq!(2, REMOTE_SFENCE_VMA_ASID);
        const_assert_eq!(3, REMOTE_HFENCE_GVMA_VMID);
        const_assert_eq!(4, REMOTE_HFENCE_GVMA);
        const_assert_eq!(5, REMOTE_HFENCE_VVMA_ASID);
        const_assert_eq!(6, REMOTE_HFENCE_VVMA);
    }
    // §9
    #[test]
    fn test_hsm() {
        use crate::hsm::*;
        const_assert_eq!(0x48534D, EID_HSM);
        const_assert_eq!(0, hart_state::STARTED);
        const_assert_eq!(1, hart_state::STOPPED);
        const_assert_eq!(2, hart_state::START_PENDING);
        const_assert_eq!(3, hart_state::STOP_PENDING);
        const_assert_eq!(4, hart_state::SUSPENDED);
        const_assert_eq!(5, hart_state::SUSPEND_PENDING);
        const_assert_eq!(6, hart_state::RESUME_PENDING);
        const_assert_eq!(0x0000_0000, suspend_type::RETENTIVE);
        const_assert_eq!(0x8000_0000, suspend_type::NON_RETENTIVE);
        const_assert_eq!(0, HART_START);
        const_assert_eq!(1, HART_STOP);
        const_assert_eq!(2, HART_GET_STATUS);
        const_assert_eq!(3, HART_SUSPEND);
    }
    // §10
    #[test]
    fn test_srst() {
        use crate::srst::*;
        const_assert_eq!(0x53525354, EID_SRST);
        const_assert_eq!(0, RESET_TYPE_SHUTDOWN);
        const_assert_eq!(1, RESET_TYPE_COLD_REBOOT);
        const_assert_eq!(2, RESET_TYPE_WARM_REBOOT);
        const_assert_eq!(0, RESET_REASON_NO_REASON);
        const_assert_eq!(1, RESET_REASON_SYSTEM_FAILURE);
        const_assert_eq!(0, SYSTEM_RESET);
    }
    // §11
    #[test]
    fn test_pmu() {
        use crate::pmu::*;
        const_assert_eq!(0x504D55, EID_PMU);
        const_assert_eq!(0, NUM_COUNTERS);
        const_assert_eq!(1, COUNTER_GET_INFO);
        const_assert_eq!(2, COUNTER_CONFIG_MATCHING);
        const_assert_eq!(3, COUNTER_START);
        const_assert_eq!(4, COUNTER_STOP);
        const_assert_eq!(5, COUNTER_FW_READ);
        const_assert_eq!(6, COUNTER_FW_READ_HI);
        const_assert_eq!(7, SNAPSHOT_SET_SHMEM);

        const_assert_eq!(0, event_type::HARDWARE_GENERAL);
        const_assert_eq!(1, event_type::HARDWARE_CACHE);
        const_assert_eq!(2, event_type::HARDWARE_RAW);
        const_assert_eq!(15, event_type::FIRMWARE);

        const_assert_eq!(0, hardware_event::NO_EVENT);
        const_assert_eq!(1, hardware_event::CPU_CYCLES);
        const_assert_eq!(2, hardware_event::INSTRUCTIONS);
        const_assert_eq!(3, hardware_event::CACHE_REFERENCES);
        const_assert_eq!(4, hardware_event::CACHE_MISSES);
        const_assert_eq!(5, hardware_event::BRANCH_INSTRUCTIONS);
        const_assert_eq!(6, hardware_event::BRANCH_MISSES);
        const_assert_eq!(7, hardware_event::BUS_CYCLES);
        const_assert_eq!(8, hardware_event::STALLED_CYCLES_FRONTEND);
        const_assert_eq!(9, hardware_event::STALLED_CYCLES_BACKEND);
        const_assert_eq!(10, hardware_event::REF_CPU_CYCLES);

        const_assert_eq!(0, cache_event::L1D);
        const_assert_eq!(1, cache_event::L1I);
        const_assert_eq!(2, cache_event::LL);
        const_assert_eq!(3, cache_event::DTLB);
        const_assert_eq!(4, cache_event::ITLB);
        const_assert_eq!(5, cache_event::BPU);
        const_assert_eq!(6, cache_event::NODE);

        const_assert_eq!(0, cache_operation::READ);
        const_assert_eq!(1, cache_operation::WRITE);
        const_assert_eq!(2, cache_operation::PREFETCH);

        const_assert_eq!(0, cache_result::ACCESS);
        const_assert_eq!(1, cache_result::MISS);

        const_assert_eq!(0, firmware_event::MISALIGNED_LOAD);
        const_assert_eq!(1, firmware_event::MISALIGNED_STORE);
        const_assert_eq!(2, firmware_event::ACCESS_LOAD);
        const_assert_eq!(3, firmware_event::ACCESS_STORE);
        const_assert_eq!(4, firmware_event::ILLEGAL_INSN);
        const_assert_eq!(5, firmware_event::SET_TIMER);
        const_assert_eq!(6, firmware_event::IPI_SENT);
        const_assert_eq!(7, firmware_event::IPI_RECEIVED);
        const_assert_eq!(8, firmware_event::FENCE_I_SENT);
        const_assert_eq!(9, firmware_event::FENCE_I_RECEIVED);
        const_assert_eq!(10, firmware_event::SFENCE_VMA_SENT);
        const_assert_eq!(11, firmware_event::SFENCE_VMA_RECEIVED);
        const_assert_eq!(12, firmware_event::SFENCE_VMA_ASID_SENT);
        const_assert_eq!(13, firmware_event::SFENCE_VMA_ASID_RECEIVED);
        const_assert_eq!(14, firmware_event::HFENCE_GVMA_SENT);
        const_assert_eq!(15, firmware_event::HFENCE_GVMA_RECEIVED);
        const_assert_eq!(16, firmware_event::HFENCE_GVMA_VMID_SENT);
        const_assert_eq!(17, firmware_event::HFENCE_GVMA_VMID_RECEIVED);
        const_assert_eq!(18, firmware_event::HFENCE_VVMA_SENT);
        const_assert_eq!(19, firmware_event::HFENCE_VVMA_RECEIVED);
        const_assert_eq!(20, firmware_event::HFENCE_VVMA_ASID_SENT);
        const_assert_eq!(21, firmware_event::HFENCE_VVMA_ASID_RECEIVED);
        const_assert_eq!(65535, firmware_event::PLATFORM);

        const_assert_eq!(4096, shmem_size::SIZE);
        const_assert_eq!(1, flags::CounterCfgFlags::SKIP_MATCH.bits());
        const_assert_eq!(2, flags::CounterCfgFlags::CLEAR_VALUE.bits());
        const_assert_eq!(4, flags::CounterCfgFlags::AUTO_START.bits());
        const_assert_eq!(8, flags::CounterCfgFlags::SET_VUINH.bits());
        const_assert_eq!(16, flags::CounterCfgFlags::SET_VSINH.bits());
        const_assert_eq!(32, flags::CounterCfgFlags::SET_UINH.bits());
        const_assert_eq!(64, flags::CounterCfgFlags::SET_SINH.bits());
        const_assert_eq!(128, flags::CounterCfgFlags::SET_MINH.bits());
        const_assert_eq!(1, flags::CounterStartFlags::INIT_VALUE.bits());
        const_assert_eq!(2, flags::CounterStartFlags::INIT_SNAPSHOT.bits());
        const_assert_eq!(1, flags::CounterStopFlags::RESET.bits());
        const_assert_eq!(2, flags::CounterStopFlags::TAKE_SNAPSHOT.bits());
    }
    // §12
    #[test]
    fn test_dbcn() {
        use crate::dbcn::*;
        const_assert_eq!(0x4442434E, EID_DBCN);
        const_assert_eq!(0, CONSOLE_WRITE);
        const_assert_eq!(1, CONSOLE_READ);
        const_assert_eq!(2, CONSOLE_WRITE_BYTE);
    }
    // §13
    #[test]
    fn test_susp() {
        use crate::susp::*;
        const_assert_eq!(0x53555350, EID_SUSP);
        const_assert_eq!(0, SUSPEND);
    }
    // §14
    #[test]
    fn test_cppc() {
        use crate::cppc::*;
        const_assert_eq!(0x43505043, EID_CPPC);
        const_assert_eq!(0, PROBE);
        const_assert_eq!(1, READ);
        const_assert_eq!(2, READ_HI);
        const_assert_eq!(3, WRITE);
    }
    // §15
    #[test]
    fn test_nacl() {
        use crate::nacl::*;
        const_assert_eq!(0x4E41434C, EID_NACL);
        const_assert_eq!(0, PROBE_FEATURE);
        const_assert_eq!(1, SET_SHMEM);
        const_assert_eq!(2, SYNC_CSR);
        const_assert_eq!(3, SYNC_HFENCE);
        const_assert_eq!(4, SYNC_SRET);

        const_assert_eq!(0, feature_id::SYNC_CSR);
        const_assert_eq!(1, feature_id::SYNC_HFENCE);
        const_assert_eq!(2, feature_id::SYNC_SRET);
        const_assert_eq!(3, feature_id::AUTOSWAP_CSR);

        const_assert_eq!(8192, shmem_size::RV32);
        const_assert_eq!(12288, shmem_size::RV64);
        const_assert_eq!(20480, shmem_size::RV128);
        match () {
            #[cfg(target_pointer_width = "32")]
            () => {
                const_assert_eq!(shmem_size::NATIVE, shmem_size::RV32);
            }
            #[cfg(target_pointer_width = "64")]
            () => {
                const_assert_eq!(shmem_size::NATIVE, shmem_size::RV64);
            }
        }
        // FIXME(2024-08-03): gate target pointer width at 128
        // Currently, values for `target_pointer_width` expected by Rustc compiler are only `16`, `32`, and `64`.
        // #[cfg(target_pointer_width = "128")]
        // () => {
        //     const_assert_eq!(shmem_size::NATIVE, shmem_size::RV128);
        // }
    }
    // §16
    #[test]
    fn test_sta() {
        use crate::sta::*;
        const_assert_eq!(0x535441, EID_STA);
        const_assert_eq!(0, SET_SHMEM);
    }
    // §17
    #[test]
    fn test_sse() {
        use crate::sse::*;
        const_assert_eq!(0x535345, EID_SSE);
        const_assert_eq!(0, READ_ATTRS);
        const_assert_eq!(1, WRITE_ATTRS);
        const_assert_eq!(2, REGISTER);
        const_assert_eq!(3, UNREGISTER);
        const_assert_eq!(4, ENABLE);
        const_assert_eq!(5, DISABLE);
        const_assert_eq!(6, COMPLETE);
        const_assert_eq!(7, INJECT);
        const_assert_eq!(8, HART_UNMASK);
        const_assert_eq!(9, HART_MASK);
    }
    // §18
    #[test]
    fn test_fwft() {
        use crate::fwft::*;
        const_assert_eq!(0x46574654, EID_FWFT);
        const_assert_eq!(0, SET);
        const_assert_eq!(1, GET);
    }
    // §19
    #[test]
    fn test_dbtr() {
        use crate::dbtr::*;
        const_assert_eq!(0x44425452, EID_DBTR);
        const_assert_eq!(0, NUM_TRIGGERS);
        const_assert_eq!(1, SET_SHMEM);
        const_assert_eq!(2, READ_TRIGGERS);
        const_assert_eq!(3, INSTALL_TRIGGERS);
        const_assert_eq!(4, UPDATE_TRIGGERS);
        const_assert_eq!(5, UNINSTALL_TRIGGERS);
        const_assert_eq!(6, ENABLE_TRIGGERS);
        const_assert_eq!(7, DISABLE_TRIGGERS);
    }
    // §20
    #[test]
    fn test_mpxy() {
        use crate::mpxy::*;
        const_assert_eq!(0x4D505859, EID_MPXY);
        const_assert_eq!(0, GET_SHMEM_SIZE);
        const_assert_eq!(1, SET_SHMEM);
        const_assert_eq!(2, GET_CHANNEL_IDS);
        const_assert_eq!(3, READ_ATTRIBUTE);
        const_assert_eq!(4, WRITE_ATTRIBUTE);
        const_assert_eq!(5, SEND_MESSAGE_WITH_RESPONSE);
        const_assert_eq!(6, SEND_MESSAGE_WITHOUT_RESPONSE);
        const_assert_eq!(7, GET_NOTIFICATION_EVENTS);
    }
}
