//! Hypervisor trap setup.
use super::super::machine::{GuestAddress, SupervisorPrivilege, Virtualization};
use crate::macros::csr;
use crate::values::trap::ExceptionSet;
#[cfg(target_pointer_width = "64")]
use crate::values::xlen::{TargetXlen, Xlen};

csr! {
    /// Hypervisor status register (`hstatus`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/hypervisor.html#hstatus>.
    Status { 0x600, r,
        /// See
        /// <https://docs.riscv.org/reference/isa/priv/hypervisor.html#hstatus>.
        ///
        /// # Safety
        ///
        /// Preserve the guest address-space and privilege context used by
        /// `sret` and by hypervisor loads and stores.
        unsafe w
    }
        GVA[6] { rw: GuestAddress }
        SPV[7] { rw: Virtualization }
        SPVP[8] { rw: SupervisorPrivilege }
        #[cfg(target_pointer_width = "64")]
        VSXL[32..=33] { r: Option<Xlen>, w: TargetXlen }
    /// Hypervisor exception delegation register (`hedeleg`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/hypervisor.html#hedeleg>.
    ExceptionDelegation { 0x602, r: ExceptionSet, w: ExceptionSet }
}
