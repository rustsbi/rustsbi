//! The build roles xtask compiles for.
//!
//! Leaf module: owns a noun, imports nothing from the pipeline.

/// What a build produces: the firmware crate, or a kernel it can embed.
/// One architecture today; a second one becomes an added variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    /// The `rustsbi-prototyper` firmware (`riscv64gc`).
    Firmware,
    /// The test/bench kernels embedded as payloads (`riscv64imac`).
    Kernel,
}

impl Target {
    /// Cargo target triple this role builds for.
    pub(crate) fn triple(self) -> &'static str {
        match self {
            Target::Firmware => "riscv64gc-unknown-none-elf",
            Target::Kernel => "riscv64imac-unknown-none-elf",
        }
    }
}
