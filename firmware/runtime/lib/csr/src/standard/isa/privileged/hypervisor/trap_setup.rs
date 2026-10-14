//! Hypervisor trap setup.
use super::super::machine::{GuestAddress, SupervisorPrivilege, Virtualization};
use crate::macros::csr;
use crate::values::trap::ExceptionSet;
#[cfg(target_pointer_width = "64")]
use crate::values::xlen::{TargetXlen, Xlen};

csr! {
    /// Hypervisor guest privilege and execution state (`hstatus`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/hypervisor.html#hstatus>
    Status { 0x600, r,
        /// See <https://docs.riscv.org/reference/isa/priv/hypervisor.html#hstatus>.
        ///
        /// # Safety
        ///
        /// Preserve the guest address-space and privilege context used by `sret`
        /// and by hypervisor loads and stores.
        unsafe w
    }
        GVA[6] { rw: GuestAddress }
        SPV[7] { rw: Virtualization }
        SPVP[8] { rw: SupervisorPrivilege }
        #[cfg(target_pointer_width = "64")]
        VSXL[32..=33] { r: Option<Xlen>, w: TargetXlen }
    /// Exceptions delegated to virtual supervisor mode (`hedeleg`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/hypervisor.html#hedeleg>
    ExceptionDelegation { 0x602, r: ExceptionSet, w: ExceptionSet }
}

impl StatusWriteValue {
    /// Creates a guest return context using the hart's XLEN and little-endian accesses.
    pub fn for_guest(privilege: SupervisorPrivilege) -> Self {
        let mut value = Self(0);
        #[cfg(target_pointer_width = "64")]
        value.write::<VSXL>(TargetXlen::X64);
        value.write::<SPV>(Virtualization::On);
        value.write::<SPVP>(privilege);
        value
    }
}
