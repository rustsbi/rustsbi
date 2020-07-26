//! base extension
use super::SbiRet;
use crate::{SBI_SPEC_MAJOR, SBI_SPEC_MINOR};
use riscv::register::{marchid, mimpid, mvendorid};

const FUNCTION_BASE_GET_SPEC_VERSION: usize = 0x0;
const FUNCTION_BASE_GET_SBI_IMPL_ID: usize = 0x1;
const FUNCTION_BASE_GET_SBI_IMPL_VERSION: usize = 0x2;
const FUNCTION_BASE_PROBE_EXTENSION: usize = 0x3;
const FUNCTION_BASE_GET_MVENDORID: usize = 0x4;
const FUNCTION_BASE_GET_MARCHID: usize = 0x5;
const FUNCTION_BASE_GET_MIMPID: usize = 0x6;

#[inline]
pub fn handle_ecall_base(function: usize, param0: usize) -> SbiRet {
    match function {
        FUNCTION_BASE_GET_SPEC_VERSION => get_spec_version(),
        FUNCTION_BASE_GET_SBI_IMPL_ID => get_sbi_impl_id(),
        FUNCTION_BASE_GET_SBI_IMPL_VERSION => get_sbi_impl_version(),
        FUNCTION_BASE_PROBE_EXTENSION => probe_extension(param0),
        FUNCTION_BASE_GET_MVENDORID => get_mvendorid(),
        FUNCTION_BASE_GET_MARCHID => get_marchid(),
        FUNCTION_BASE_GET_MIMPID => get_mimpid(),
        _ => SbiRet::not_supported(),
    }
}

#[inline]
fn get_spec_version() -> SbiRet {
    let spec_version = (SBI_SPEC_MAJOR << 24) | (SBI_SPEC_MINOR);
    SbiRet::ok(spec_version)
}

#[inline]
fn get_sbi_impl_id() -> SbiRet {
    let sbi_impl_id = crate::IMPL_ID_RUSTSBI;
    SbiRet::ok(sbi_impl_id)
}

#[inline]
fn get_sbi_impl_version() -> SbiRet {
    let sbi_impl_version = crate::RUSTSBI_VERSION;
    SbiRet::ok(sbi_impl_version)
}

#[inline]
fn probe_extension(extension_id: usize) -> SbiRet {
    const NO_EXTENSION: usize = 0;
    const HAS_EXTENSION: usize = 1;
    let ans = crate::extension::probe_extension(extension_id);
    SbiRet::ok(if ans { HAS_EXTENSION } else { NO_EXTENSION })
}

#[inline]
fn get_mvendorid() -> SbiRet {
    SbiRet::ok(mvendorid::read().map(|r| r.bits()).unwrap_or(0))
}

#[inline]
fn get_marchid() -> SbiRet {
    SbiRet::ok(marchid::read().map(|r| r.bits()).unwrap_or(0))
}

#[inline]
fn get_mimpid() -> SbiRet {
    SbiRet::ok(mimpid::read().map(|r| r.bits()).unwrap_or(0))
}
