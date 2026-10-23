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
        Self::all()
            .iter_names()
            .filter(|(name, _)| {
                if name.len() == 1 {
                    base.get(4..)
                        .unwrap_or_default()
                        .as_bytes()
                        .iter()
                        .any(|byte| byte.eq_ignore_ascii_case(&name.as_bytes()[0]))
                } else {
                    parts.clone().any(|part| {
                        let Some(prefix) = part.get(..name.len()) else {
                            return false;
                        };
                        if !prefix.eq_ignore_ascii_case(name) {
                            return false;
                        }
                        let version = &part[name.len()..];
                        if version.is_empty() {
                            return true;
                        }
                        let (major, minor) = version
                            .split_once(['p', 'P'])
                            .map_or((version, None), |(major, minor)| (major, Some(minor)));
                        !major.is_empty()
                            && major.bytes().all(|byte| byte.is_ascii_digit())
                            && minor.is_none_or(|minor| {
                                !minor.is_empty() && minor.bytes().all(|byte| byte.is_ascii_digit())
                            })
                    })
                }
            })
            .map(|(_, flag)| flag)
            .collect()
    }
}
