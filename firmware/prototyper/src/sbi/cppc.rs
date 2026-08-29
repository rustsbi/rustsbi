use rustsbi::SbiRet;

/// Implementation of SBI CPPC extension.
///
/// No CPPC register backend is discovered by the prototyper yet, so all
/// register probes report a zero width and accesses are rejected.
pub(crate) struct SbiCppc;

impl SbiCppc {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl rustsbi::Cppc for SbiCppc {
    fn probe(&self, _reg_id: u32) -> SbiRet {
        SbiRet::success(0)
    }

    fn read(&self, _reg_id: u32) -> SbiRet {
        SbiRet::not_supported()
    }

    fn read_hi(&self, _reg_id: u32) -> SbiRet {
        SbiRet::not_supported()
    }

    fn write(&self, _reg_id: u32, _val: u64) -> SbiRet {
        SbiRet::not_supported()
    }
}
