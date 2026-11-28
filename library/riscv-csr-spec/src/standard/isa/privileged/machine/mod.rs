//! Machine-level CSR definitions.
//!
//! The four [machine state-enable registers](https://docs.riscv.org/reference/isa/v20260120/priv/smstateen.html#3-1-1-state-enable-extensions)
//! control lower-privilege access to architectural state. Each `SE0` bit governs
//! access to the corresponding `sstateenN` and `hstateenN`.
//!
//! [`mcountinhibit`](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#2-1-1-12-machine-counter-inhibit-mcountinhibit-register)
//! controls which [hardware performance counters](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#2-1-1-10-hardware-performance-monitor)
//! can increment.
//!
//! In [`mstatus`/`mstatush`](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#2-1-1-6-machine-status-mstatus-and-mstatush-registers),
//! a saved VS-mode context has `MPP = Supervisor` and `MPV = 1`.
//! [`medeleg` and `mideleg`](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#2-1-1-8-machine-trap-delegation-medeleg-and-mideleg-registers)
//! select exceptions and interrupts for delegation. Coordinate writes to
//! `mcause`, `mtval`, `mtinst` and `mtval2` with M-mode trap handling.
//!
//! After a [PMP update](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#norm:pmp_sfence_required)
//! on a hart with page-based translation, execute `SFENCE.VMA x0, x0` for local
//! S/HS translations. With [two-stage translation](https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#two-stage-translation),
//! `HFENCE.GVMA x0, x0` also synchronizes cached guest translations with the update.

mod configuration;
mod counter_setup;
mod counter_timer;
mod indirect;
mod information;
mod pmp;
mod state_enable;
mod trap_handling;
mod trap_setup;

pub use configuration::*;
pub use counter_setup::*;
pub use counter_timer::*;
pub use indirect::*;
pub use information::*;
pub use pmp::*;
pub use state_enable::*;
pub use trap_handling::*;
pub use trap_setup::*;
