//! CLINT backend selection from FDT identity and register layout.

/// The CLINT device kinds the firmware can drive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClintKind {
    Sifive,
    THead,
}

const SIFIVE_CLINT_COMPATIBLES: [&str; 3] =
    ["riscv,clint0", "starfive,jh7110-clint", "sifive,clint0"];
const T_HEAD_CLINT_COMPATIBLES: [&str; 1] = ["thead,c900-clint"];

impl ClintKind {
    /// Maps one `compatible` string and the node's access-width property to a
    /// driver kind.
    pub(crate) fn from_fdt(compatible: &str, has_no_64bit_mmio: bool) -> Option<Self> {
        if SIFIVE_CLINT_COMPATIBLES.contains(&compatible) {
            Some(if has_no_64bit_mmio {
                Self::THead
            } else {
                Self::Sifive
            })
        } else if T_HEAD_CLINT_COMPATIBLES.contains(&compatible) {
            Some(Self::THead)
        } else {
            None
        }
    }

    /// Device name reported in boot logs.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Sifive => "SiFiveClint",
            Self::THead => "THeadClint",
        }
    }
}
