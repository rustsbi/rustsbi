//! State-enable registers used to prepare a supervisor next stage.

use super::FeatureError;
use crate::csr::{
    HypervisorState, HypervisorStateHigh, MachineState, MachineStateHigh, Readable, StateEnable,
    StateEnableHigh, SupervisorState, Value, Writable,
};

pub(super) fn configure_supervisor(aia_access: bool) -> Result<(), FeatureError> {
    if MachineState::<0>::read_optional()
        .map_err(FeatureError::Access)?
        .is_none()
    {
        return Ok(());
    }
    let mut access = StateEnable::STATE_ENABLE | StateEnable::CONTEXT | StateEnable::ENVIRONMENT;
    if aia_access {
        access |= StateEnable::IMSIC | StateEnable::AIA | StateEnable::INDIRECT_SUPERVISOR;
    }
    // CTR remains disabled until policy independently detects Ssctr.
    write64::<MachineState<0>, MachineStateHigh<0>>(access)?;
    write64::<MachineState<1>, MachineStateHigh<1>>(StateEnable::STATE_ENABLE)?;
    write64::<MachineState<2>, MachineStateHigh<2>>(StateEnable::STATE_ENABLE)?;
    write64::<MachineState<3>, MachineStateHigh<3>>(StateEnable::STATE_ENABLE)?;

    if SupervisorState::<0>::read_optional()
        .map_err(FeatureError::Access)?
        .is_some()
    {
        SupervisorState::<0>::write(0).map_err(FeatureError::Access)?;
        SupervisorState::<1>::write(0).map_err(FeatureError::Access)?;
        SupervisorState::<2>::write(0).map_err(FeatureError::Access)?;
        SupervisorState::<3>::write(0).map_err(FeatureError::Access)?;
    }
    if HypervisorState::<0>::read_optional()
        .map_err(FeatureError::Access)?
        .is_some()
    {
        write64::<HypervisorState<0>, HypervisorStateHigh<0>>(0)?;
        write64::<HypervisorState<1>, HypervisorStateHigh<1>>(0)?;
        write64::<HypervisorState<2>, HypervisorStateHigh<2>>(0)?;
        write64::<HypervisorState<3>, HypervisorStateHigh<3>>(0)?;
    }
    Ok(())
}

fn write64<Low, High>(value: u64) -> Result<(), FeatureError>
where
    Low: Writable<Value = StateEnable>,
    High: Writable<Value = StateEnableHigh>,
{
    Low::write(StateEnable::from_bits(value as usize)).map_err(FeatureError::Access)?;
    #[cfg(target_pointer_width = "32")]
    High::write(StateEnableHigh::from_bits((value >> 32) as usize))
        .map_err(FeatureError::Access)?;
    Ok(())
}
