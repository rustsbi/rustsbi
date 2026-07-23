//! Ordinary device bindings awaiting extraction to sibling driver crates.

pub mod sifive_test;
pub mod uart;

/// Failure while validating or constructing an ordinary device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DriverError {
    /// The owned boot tree is malformed or an exact selected node is absent.
    DeviceTree,
    /// The selected node does not describe a supported device.
    Unsupported,
    /// The selected register range is malformed or too short.
    InvalidRange,
    /// The description is outside the configured trusted platform.
    Unauthorized,
    /// Another live machine resource already owns the register range.
    AlreadyOwned,
    /// A required device access failed.
    Hardware,
}
