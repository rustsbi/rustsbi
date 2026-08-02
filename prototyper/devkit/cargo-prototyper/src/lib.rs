//! Host-side construction and execution contracts for Prototyper images.
//!
//! Target firmware crates never depend on this package. CLI parsing resolves
//! into immutable plans before Cargo, artifact conversion, or QEMU starts.

mod artifact;
mod error;
mod manifest;
mod plan;
mod project;
mod runner;

pub use artifact::{ArtifactSet, execute};
pub use error::Error;
pub use manifest::{
    FirmwareConfig, FirmwareManifest, FirmwareType, NextMode, NextStageManifest, NextStageSource,
};
pub use plan::{
    Architecture, BuildOptions, CargoAction, DEFAULT_FIRMWARE_TARGET, DEFAULT_PAYLOAD_TARGET,
    ExecutionPlan, ImagePlan, ImageRole, LinkInput, LinkInputContents, RV32_FIRMWARE_TARGET,
    RV32_PAYLOAD_TARGET,
};
pub use project::Project;
pub use runner::{LaunchPlan, Outcome, QemuConfig, QemuMachine, SerialMode, run as run_launch};
