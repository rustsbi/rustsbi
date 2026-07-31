//! Hart identity, lifecycle, notification, and remote-fence capabilities.

mod admission;
mod control;
mod fence;
mod instructions;
mod ipi;
mod local;
mod lock;
pub(crate) mod protocol;
mod start;
mod warm;

pub use control::{HartControl, HartError};
pub use fence::{RemoteFence, RemoteFenceError};
pub use ipi::{Ipi, IpiError};
pub use local::{HartLocal, HartLocalError, HartLocalGuard, HartTargets};
pub use sbi_spec::hsm::HartState;

pub(crate) use ipi::{IpiDevice, Notification};
pub(crate) use local::{entry_index, publish, resolve};
pub(crate) use protocol::{HartAdmission, notify_terminal_peers};
pub(crate) use warm::run as run_warm_hart;
