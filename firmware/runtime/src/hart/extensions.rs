//! Immutable ISA-extension facts stored with each Runtime hart.

use bitflags::bitflags;

bitflags! {
    /// ISA extensions reported by platform discovery for one hart.
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct IsaExtensions: u16 {
        /// Supervisor timer comparison.
        const SSTC = 1 << 0;
        /// Hypervisor privilege mode.
        const H = 1 << 1;
        /// Machine advanced interrupt architecture.
        const SMAIA = 1 << 2;
        /// Page-based memory types.
        const SVPBMT = 1 << 3;
        /// Entropy-source access.
        const ZKR = 1 << 4;
        /// Cache-block management operations.
        const ZICBOM = 1 << 5;
        /// Cache-block zero operations.
        const ZICBOZ = 1 << 6;
        /// Forward-edge control-flow integrity.
        const ZICFILP = 1 << 7;
        /// Backward-edge control-flow integrity.
        const ZICFISS = 1 << 8;
        /// Supervisor double-trap handling.
        const SSDBLTRP = 1 << 9;
        /// Page faults on unset PTE accessed or dirty bits.
        const SVADE = 1 << 10;
        /// Hardware updates of PTE accessed and dirty bits.
        const SVADU = 1 << 11;
        /// Supervisor pointer masking.
        const SMNPM = 1 << 12;
        /// Supervisor indirect CSR access.
        const SSCSRIND = 1 << 13;
        /// Supervisor advanced interrupt architecture.
        const SSAIA = 1 << 14;
    }
}

impl IsaExtensions {
    pub(crate) fn from_cpu(cpu: crate::FdtNode<'_, '_>) -> Self {
        if let Some(property) = cpu.property("riscv,isa-extensions") {
            Self::all()
                .iter_names()
                .filter(|(name, _)| {
                    property
                        .value
                        .split(|byte| *byte == 0)
                        .any(|value| name.as_bytes().eq_ignore_ascii_case(value))
                })
                .map(|(_, flag)| flag)
                .collect()
        } else {
            cpu.property("riscv,isa")
                .and_then(|property| property.as_str())
                .map(Self::from_isa)
                .unwrap_or_default()
        }
    }

    fn from_isa(isa: &str) -> Self {
        let mut parts = isa.split('_');
        let base = parts.next().unwrap_or_default();
        let letters = base.get(4..).unwrap_or_default().as_bytes();
        let mut extensions = Self::empty();
        for (name, flag) in Self::all().iter_names() {
            let present = match name.as_bytes() {
                [letter] => letters.iter().any(|byte| byte.eq_ignore_ascii_case(letter)),
                _ => parts.clone().any(|part| matches_extension(part, name)),
            };
            if present {
                extensions.insert(flag);
            }
        }
        extensions
    }
}

fn matches_extension(part: &str, name: &str) -> bool {
    let Some((prefix, version)) = part.split_at_checked(name.len()) else {
        return false;
    };
    prefix.eq_ignore_ascii_case(name) && valid_version(version)
}

fn valid_version(version: &str) -> bool {
    match version.split_once(['p', 'P']) {
        None => version.bytes().all(|byte| byte.is_ascii_digit()),
        Some((major, minor)) => {
            !major.is_empty()
                && !minor.is_empty()
                && major
                    .bytes()
                    .chain(minor.bytes())
                    .all(|byte| byte.is_ascii_digit())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::IsaExtensions;

    #[test]
    fn recognizes_base_letters_and_versioned_extensions() {
        assert_eq!(
            IsaExtensions::from_isa("rv64imafdh_sstc1p0_svpbmt1_zicbom"),
            IsaExtensions::H | IsaExtensions::SSTC | IsaExtensions::SVPBMT | IsaExtensions::ZICBOM
        );
        assert_eq!(
            IsaExtensions::from_isa("RV32IH_SSTC1P0_ZICBOZ"),
            IsaExtensions::H | IsaExtensions::SSTC | IsaExtensions::ZICBOZ
        );
    }

    #[test]
    fn rejects_prefix_matches_and_incomplete_versions() {
        for isa in [
            "rv64i_sstcextra",
            "rv64i_sstcp0",
            "rv64i_sstc1p",
            "rv64i_sstc1p0p1",
            "rv64i_sstc1x",
            "rv64i_sstc-1",
        ] {
            assert!(IsaExtensions::from_isa(isa).is_empty(), "{isa}");
        }
    }

    #[test]
    fn handles_short_unknown_and_non_ascii_parts() {
        for isa in ["", "rv", "rv64i__unknown_", "rv64i_ssté", "rv64i_sstc１"] {
            assert!(IsaExtensions::from_isa(isa).is_empty(), "{isa}");
        }
        assert_eq!(
            IsaExtensions::from_isa("rv64i_sstc_sstc"),
            IsaExtensions::SSTC
        );
    }
}
