//! Initial state-access permissions owned by the current hart.

use crate::{
    Error, Result,
    hart::{Hart, HartId, IsaExtensions},
    recovery::{csrr, csrw},
};
use bitflags::bitflags;
use csr::machine::TargetPrivilege;
use csr::{hypervisor, machine, supervisor, values::StateEnableSet};
#[cfg(target_pointer_width = "64")]
use machine::{
    AIA, CONTEXT, CSRIND, ENVCFG, IMSIC, SE0, StateEnable0 as Permissions,
    StateEnable0ReadValue as PermissionsReadValue, StateEnable0WriteValue as PermissionsValue,
};
#[cfg(target_pointer_width = "32")]
use machine::{
    AIAH as AIA, CONTEXTH as CONTEXT, CSRINDH as CSRIND, ENVCFGH as ENVCFG, IMSICH as IMSIC,
    SE0H as SE0, StateEnable0High as Permissions,
    StateEnable0HighReadValue as PermissionsReadValue,
    StateEnable0HighWriteValue as PermissionsValue,
};

bitflags! {
    #[derive(Clone, Copy)]
    struct RegisterGroups: u8 {
        const MACHINE = 1 << 0;
        const SUPERVISOR = 1 << 1;
        const HYPERVISOR = 1 << 2;
    }
}

/// State-enable register groups discovered for this hart's M-mode startup.
/// Permissions remain in the hardware CSRs; this record only caches availability.
pub struct StateEnable {
    implemented: RegisterGroups,
}

impl Hart {
    /// Returns this hart's discovered state-enable support, or None before setup.
    pub fn state_enable(&self) -> Option<&StateEnable> {
        self.state_enable.get()
    }

    /// Discovers and configures state-enable registers on the current hart.
    /// Initial boot setup owns these permissions and excludes local IRQs.
    pub(crate) fn initialize_state_enable(&self) -> Result<()> {
        if HartId::current()? != self.id() {
            return Err(Error::InvalidArgs);
        }
        let state_enable = self
            .state_enable
            .try_call_once(|| StateEnable::discover(self))?;
        state_enable.configure(self)
    }
}

impl StateEnable {
    /// Returns whether this hart implements machine state-enable registers.
    pub fn is_available(&self) -> bool {
        self.implemented.contains(RegisterGroups::MACHINE)
    }

    fn discover(hart: &Hart) -> Result<Self> {
        let mut implemented = RegisterGroups::empty();
        match csrr!(machine::StateEnable0) {
            Ok(_) => implemented.insert(RegisterGroups::MACHINE),
            Err(Error::IllegalInstruction) => return Ok(Self { implemented }),
            Err(error) => return Err(error),
        }
        if hart.supports_privilege(TargetPrivilege::Supervisor) {
            match csrr!(supervisor::StateEnable0) {
                Ok(_) => implemented.insert(RegisterGroups::SUPERVISOR),
                Err(Error::IllegalInstruction) => {}
                Err(error) => return Err(error),
            }
        }
        if hart.has_hypervisor() {
            match csrr!(hypervisor::StateEnable0) {
                Ok(_) => implemented.insert(RegisterGroups::HYPERVISOR),
                Err(Error::IllegalInstruction) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(Self { implemented })
    }

    fn configure(&self, hart: &Hart) -> Result<()> {
        if !self.is_available() {
            return Ok(());
        }
        Self::configure_machine_access(hart)?;
        Self::delegate_registers()?;
        if self.implemented.contains(RegisterGroups::SUPERVISOR) {
            Self::close_supervisor_access()?;
        }
        if self.implemented.contains(RegisterGroups::HYPERVISOR) {
            Self::close_hypervisor_access()?;
        }
        Ok(())
    }

    fn configure_machine_access(hart: &Hart) -> Result<()> {
        let aia = hart.extensions().contains(IsaExtensions::SSAIA);

        let indirect = aia || hart.extensions().contains(IsaExtensions::SSCSRIND);
        let mut permissions = PermissionsValue::closed();
        permissions.write::<SE0>(machine::FeatureState::Enabled);
        permissions.write::<CONTEXT>(machine::FeatureState::Enabled);
        permissions.write::<ENVCFG>(machine::FeatureState::Enabled);
        permissions.write::<AIA>(aia.into());
        // Supervisor IMSIC access is independent of the machine IPI transport.
        // Unimplemented state remains WARL zero on platforms without IMSIC.
        permissions.write::<IMSIC>(aia.into());
        permissions.write::<CSRIND>(indirect.into());
        #[cfg(target_pointer_width = "32")]
        csrw!(
            machine::StateEnable0,
            usize::from(machine::StateEnable0WriteValue::closed())
        )?;
        csrw!(Permissions, usize::from(permissions))?;
        let actual = csrr!(Permissions).map(PermissionsReadValue::from_bits)?;
        if (aia && actual.read::<AIA>() != machine::FeatureState::Enabled)
            || (indirect && actual.read::<CSRIND>() != machine::FeatureState::Enabled)
        {
            return Err(Error::NotSupported);
        }
        Ok(())
    }

    fn delegate_registers() -> Result<()> {
        seq_macro::seq!(N in 1..=3 {
            csrw!(machine::StateEnable~N, usize::from(StateEnableSet::lower_registers()))?;
        });
        #[cfg(target_pointer_width = "32")]
        {
            use machine::{
                StateEnable1High as High1, StateEnable2High as High2, StateEnable3High as High3,
            };
            seq_macro::seq!(N in 1..=3 {
                csrw!(High~N, usize::from(StateEnableSet::lower_registers_high()))?;
            });
        }
        Ok(())
    }

    fn close_supervisor_access() -> Result<()> {
        seq_macro::seq!(N in 0..=3 {
            csrw!(supervisor::StateEnable~N, usize::from(StateEnableSet::closed()))?;
        });
        // Read-only-one bits need not read back as numeric zero.
        Ok(())
    }

    fn close_hypervisor_access() -> Result<()> {
        seq_macro::seq!(N in 0..=3 {
            csrw!(hypervisor::StateEnable~N, usize::from(StateEnableSet::closed()))?;
        });
        #[cfg(target_pointer_width = "32")]
        {
            use hypervisor::{
                StateEnable0High as High0, StateEnable1High as High1, StateEnable2High as High2,
                StateEnable3High as High3,
            };
            seq_macro::seq!(N in 0..=3 {
                csrw!(High~N, usize::from(StateEnableSet::closed()))?;
            });
        }
        Ok(())
    }
}
