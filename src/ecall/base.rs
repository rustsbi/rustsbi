use sbi_spec::binary::SbiRet;

#[inline]
pub(super) fn handle_ecall(function: usize, param0: usize) -> SbiRet {
    use crate::base::*;
    use crate::{IMPL_ID_RUSTSBI, RUSTSBI_VERSION, SBI_SPEC_MAJOR, SBI_SPEC_MINOR};
    use riscv::register::{marchid, mimpid, mvendorid};
    use sbi_spec::base::*;

    let value = match function {
        GET_SBI_SPEC_VERSION => (SBI_SPEC_MAJOR << 24) | (SBI_SPEC_MINOR),
        GET_SBI_IMPL_ID => IMPL_ID_RUSTSBI,
        GET_SBI_IMPL_VERSION => RUSTSBI_VERSION,
        PROBE_EXTENSION => {
            if probe_extension(param0) {
                UNAVAILABLE_EXTENSION.wrapping_add(1)
            } else {
                UNAVAILABLE_EXTENSION
            }
        }
        GET_MVENDORID => mvendorid::read().map(|r| r.bits()).unwrap_or(0),
        GET_MARCHID => marchid::read().map(|r| r.bits()).unwrap_or(0),
        GET_MIMPID => mimpid::read().map(|r| r.bits()).unwrap_or(0),
        _ => return SbiRet::not_supported(),
    };
    SbiRet::success(value)
}
