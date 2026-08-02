use std::fmt;
use std::io;
use std::path::PathBuf;

/// Failure reported before or while executing a resolved development plan.
#[derive(Debug)]
pub enum Error {
    WorkspaceNotFound(PathBuf),
    UnsupportedTarget(String),
    UnsupportedPlatform(String),
    UnsupportedRoleTarget {
        role: &'static str,
        target: String,
    },
    UnsupportedPackaging(&'static str),
    UnsupportedLaunchRole(&'static str),
    InvalidRunnerConfiguration(&'static str),
    ConflictingStageSources,
    InvalidManifest(String),
    InvalidFirmwareContract(&'static str),
    InvalidLinkAddress(String),
    MissingInput(PathBuf),
    Io {
        action: &'static str,
        source: io::Error,
    },
    ProcessFailed {
        program: String,
        code: Option<i32>,
    },
}

impl Error {
    pub fn io(action: &'static str, source: io::Error) -> Self {
        Self::Io { action, source }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorkspaceNotFound(path) => {
                write!(
                    formatter,
                    "could not find the RustSBI workspace above {}",
                    path.display()
                )
            }
            Self::UnsupportedTarget(target) => {
                write!(formatter, "unsupported Prototyper target `{target}`")
            }
            Self::UnsupportedPlatform(platform) => {
                write!(formatter, "unsupported Prototyper platform `{platform}`")
            }
            Self::UnsupportedRoleTarget { role, target } => {
                write!(formatter, "image role `{role}` does not support `{target}`")
            }
            Self::UnsupportedPackaging(role) => {
                write!(
                    formatter,
                    "image role `{role}` cannot be packaged as a FIT image"
                )
            }
            Self::UnsupportedLaunchRole(role) => {
                write!(
                    formatter,
                    "image role `{role}` cannot be launched as QEMU firmware"
                )
            }
            Self::InvalidRunnerConfiguration(reason) => {
                write!(formatter, "invalid QEMU configuration: {reason}")
            }
            Self::ConflictingStageSources => {
                formatter.write_str("external and embedded next stages are mutually exclusive")
            }
            Self::InvalidManifest(reason) => {
                write!(formatter, "invalid Prototyper.toml: {reason}")
            }
            Self::InvalidFirmwareContract(reason) => {
                write!(formatter, "invalid firmware contract: {reason}")
            }
            Self::InvalidLinkAddress(value) => {
                write!(
                    formatter,
                    "test link address `{value}` must be page-aligned hexadecimal"
                )
            }
            Self::MissingInput(path) => {
                write!(
                    formatter,
                    "required input does not exist: {}",
                    path.display()
                )
            }
            Self::Io { action, source } => write!(formatter, "{action}: {source}"),
            Self::ProcessFailed { program, code } => match code {
                Some(code) => write!(formatter, "`{program}` exited with status {code}"),
                None => write!(formatter, "`{program}` was terminated by a signal"),
            },
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
