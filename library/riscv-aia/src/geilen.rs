//! Guest external interrupt line number configuration.
//!
//! This module provides configuration for guest interrupt files per hart.
//! Note: GEILEN is not implemented as a hardware CSR in this library.

/// Maximum number of guest interrupt files per hart supported by GEILEN.
pub const MAX_GUEST_FILES_PER_HART: u32 = 63;

/// Guest external interrupt line number configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geilen {
    /// Number of guest interrupt files per hart (0-63).
    guest_files_per_hart: u32,
}

impl Geilen {
    /// Create a new Geilen configuration.
    pub const fn new(guest_files_per_hart: u32) -> Self {
        assert!(guest_files_per_hart <= MAX_GUEST_FILES_PER_HART);
        Self {
            guest_files_per_hart,
        }
    }

    /// Get the number of guest interrupt files per hart.
    pub const fn guest_files_per_hart(self) -> u32 {
        self.guest_files_per_hart
    }

    /// Set the number of guest interrupt files per hart (0-63).
    pub const fn set_guest_files_per_hart(mut self, count: u32) -> Self {
        assert!(count <= MAX_GUEST_FILES_PER_HART);
        self.guest_files_per_hart = count;
        self
    }

    /// Create a Geilen with maximum guest files per hart.
    pub const fn max() -> Self {
        Self::new(MAX_GUEST_FILES_PER_HART)
    }

    /// Create a Geilen with no guest files per hart.
    pub const fn none() -> Self {
        Self::new(0)
    }
}

impl Default for Geilen {
    fn default() -> Self {
        Self::max()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geilen_guest_files() {
        let reg = Geilen::new(42);
        assert_eq!(reg.guest_files_per_hart(), 42);

        let modified = reg.set_guest_files_per_hart(63);
        assert_eq!(modified.guest_files_per_hart(), 63);

        let zero = reg.set_guest_files_per_hart(0);
        assert_eq!(zero.guest_files_per_hart(), 0);
    }

    #[test]
    fn geilen_constants() {
        let max = Geilen::max();
        assert_eq!(max.guest_files_per_hart(), MAX_GUEST_FILES_PER_HART);

        let none = Geilen::none();
        assert_eq!(none.guest_files_per_hart(), 0);
    }

    #[test]
    fn geilen_default() {
        let default = Geilen::default();
        assert_eq!(default.guest_files_per_hart(), MAX_GUEST_FILES_PER_HART);
    }

    #[test]
    #[should_panic]
    fn geilen_invalid_count() {
        let _ = Geilen::new(64); // Should panic
    }
}
