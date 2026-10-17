//! Enabled hart descriptions and current-hart initialization.

use super::state::HartStateCell;
use super::{HartId, HartLocal, IsaExtensions, StateEnable};
use crate::recovery::csrr;
use crate::{Error, PlatformView, Result, node_is_enabled};
use alloc::vec::Vec;
use csr::machine;
use csr::machine::TargetPrivilege;
use spin::Once;

static HARTS: HartLocal<Hart> = HartLocal::new();

/// An enabled hart with immutable platform facts and Runtime-owned hardware facts.
pub struct Hart {
    id: HartId,
    extensions: IsaExtensions,
    hardware: Once<HardwareFacts>,
    pub(super) state_enable: Once<StateEnable>,
    pub(super) state: HartStateCell,
}

struct HardwareFacts {
    isa: machine::IsaValue,
    privileged_version: PrivilegedVersion,
}

/// Privileged-architecture version inferred from implemented standard CSRs.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum PrivilegedVersion {
    /// No recognized version was detected.
    #[default]
    Unknown,
    /// Machine counter-enable is available.
    Version1_10,
    /// Machine counter-inhibit is also available.
    Version1_11,
    /// Machine environment configuration is also available.
    Version1_12,
}

impl HardwareFacts {
    fn read() -> Self {
        let privileged_version = if csrr!(machine::CounterEnable).is_err() {
            PrivilegedVersion::Unknown
        } else if csrr!(machine::CounterInhibit).is_err() {
            PrivilegedVersion::Version1_10
        } else if csrr!(machine::EnvironmentConfig).is_err() {
            PrivilegedVersion::Version1_11
        } else {
            PrivilegedVersion::Version1_12
        };
        Self {
            isa: machine::Isa::read(),
            privileged_version,
        }
    }
}

impl Hart {
    pub(crate) fn discover(platform: PlatformView<'_>) -> Result<()> {
        let cpus = platform
            .find_enabled_node("/cpus")
            .ok_or(Error::InvalidArgs)?;
        let mut harts: Vec<_> = cpus
            .children()
            .filter(|node| node.name.split('@').next() == Some("cpu") && node_is_enabled(*node))
            .map(|node| {
                Ok(Self {
                    id: HartId::from_raw(PlatformView::hart_id(node)?)?,
                    extensions: IsaExtensions::from_cpu(node),
                    hardware: Once::new(),
                    state_enable: Once::new(),
                    state: HartStateCell::new(),
                })
            })
            .collect::<Result<_>>()?;
        harts.sort_unstable_by_key(|hart| hart.id);
        if harts.len() != HartId::count()
            || harts
                .iter()
                .zip(HartId::all())
                .any(|(hart, id)| hart.id != id)
        {
            return Err(Error::InvalidArgs);
        }
        let mut harts = harts.into_iter();
        HARTS.init_all(|_| harts.next().expect("validated hart descriptions"))
    }

    /// Borrows the description of a validated enabled hart.
    pub fn get(id: HartId) -> Result<&'static Self> {
        HARTS.get(id)
    }

    /// Borrows this hart's description after topology discovery.
    pub fn current() -> Result<&'static Self> {
        Self::get(HartId::current()?)
    }

    /// Iterates enabled hart descriptions in hardware-ID order.
    pub fn all() -> impl DoubleEndedIterator<Item = &'static Self> {
        HARTS.iter()
    }

    /// Returns this hart's validated identifier.
    pub const fn id(&self) -> HartId {
        self.id
    }

    /// Returns immutable ISA extensions reported by platform discovery.
    pub const fn extensions(&self) -> &IsaExtensions {
        &self.extensions
    }

    /// Returns actual H support once this hart has begun initialization.
    pub fn has_hypervisor(&self) -> bool {
        self.hardware
            .get()
            .is_some_and(|facts| facts.isa.has_hypervisor())
    }

    /// Returns privilege support observed on this hart during initialization.
    pub fn supports_privilege(&self, mode: TargetPrivilege) -> bool {
        mode == TargetPrivilege::Machine
            || self.hardware.get().is_some_and(|facts| match mode {
                TargetPrivilege::Supervisor => facts.isa.has_supervisor(),
                TargetPrivilege::User => facts.isa.has_user(),
                TargetPrivilege::Machine => true,
            })
    }

    /// Returns the cached privileged-architecture version, or Unknown before initialization.
    pub fn privileged_version(&self) -> PrivilegedVersion {
        self.hardware
            .get()
            .map_or(PrivilegedVersion::Unknown, |facts| facts.privileged_version)
    }

    /// Initializes this hart's Runtime bindings for the requested next-stage privilege.
    /// Remote calls fail before hardware access. Runtime excludes IRQs and permits
    /// successful initialization once; a failed hardware configuration prevents handoff.
    pub fn initialize(&'static self, next_mode: TargetPrivilege) -> Result<()> {
        if HartId::current()? != self.id {
            return Err(Error::InvalidArgs);
        }
        if !crate::boot::traps_ready() || usize::from(machine::Scratch::read()) != 0 {
            return Err(Error::NotReady);
        }
        crate::irq::with_local_disabled(|_| {
            self.hardware.call_once(HardwareFacts::read);
            if !self.supports_privilege(next_mode) {
                return Err(Error::NotSupported);
            }
            crate::boot::configure_next_stage(self)
        })
    }
}
