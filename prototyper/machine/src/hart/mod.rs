//! Hart identity, lifecycle, notification, and remote-fence capabilities.

mod arch;
mod control;
mod fence;
mod ipi;
mod lock;
mod map;
pub(crate) mod runtime;
mod start;
mod state;

pub use control::{HartControl, HartError, HartStatus};
pub use fence::{RemoteFence, RemoteFenceError};
pub use ipi::{Ipi, IpiError};
pub use map::{HartLocal, HartLocalError, HartLocalGuard, HartTargets};

pub(crate) use ipi::{IpiDevice, Notification};
pub(crate) use map::{entry_index, publish, resolve};
pub(crate) use runtime::{HartRuntime, notify_terminal_peers};
