//! Machine trap entry, dispatch, instruction emulation, and S/HS/VS forwarding.
//! Return state and original trap facts remain private to this subsystem.
//! Dispatch owns recovery and forwarding; emulation commits successful
//! instructions using the timer service or bounded lower-mode memory access.

use riscv_csr_spec::standard::isa::values::{
    address::Address,
    trap::{CauseValue, TrapValueBits},
};

cfg_select! {
    any(target_arch = "riscv32", target_arch = "riscv64") => {
        mod access;
        mod decode;
        mod dispatch;
        mod emulate;
        pub(crate) mod entry;
        mod frame;
        mod handlers;
        mod memory;
        mod redirect;
        pub use dispatch::current_fault_snapshot;

        use riscv_csr_spec::standard::isa::values::trap::Exception;

        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        struct Fault {
            cause: Exception,
            address: Address,
        }

        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        enum Error {
            Unsupported,
            Fault(Fault),
            Runtime(crate::Error),
        }
    }
    test => {
        mod decode;
    }
    _ => {}
}

/// Raw machine-trap CSR state read on the current hart for fatal diagnostics.
#[derive(Clone, Copy, Debug)]
pub struct FaultSnapshot {
    /// Interrupt flag and cause code from `mcause`.
    pub mcause: CauseValue,
    /// Interrupted program counter from `mepc`.
    pub mepc: Address,
    /// Additional exception information from `mtval`, interpreted according to `mcause`.
    pub mtval: TrapValueBits,
}
