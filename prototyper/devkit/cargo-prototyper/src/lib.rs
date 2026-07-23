//! Host-side construction and execution contracts for Prototyper images.
//!
//! Target firmware crates never depend on this package. CLI parsing resolves
//! into immutable plans before Cargo, artifact conversion, or QEMU starts.

mod artifact;
mod error;
mod plan;
mod project;
mod runner;

pub use artifact::{ArtifactSet, execute};
pub use error::Error;
pub use plan::{
    BuildOptions, CargoAction, DEFAULT_FIRMWARE_TARGET, DEFAULT_PAYLOAD_TARGET, ExecutionPlan,
    ImagePlan, ImageRole,
};
pub use project::Project;
pub use runner::{LaunchPlan, Outcome, QemuConfig, QemuMachine, SerialMode, run as run_launch};
