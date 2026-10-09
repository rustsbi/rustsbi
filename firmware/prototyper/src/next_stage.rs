//! Next-stage selection for dynamic, jump and embedded-payload firmware.
//!
//! Selection retains the raw entry for stack exclusion. Dynamic policy errors
//! remain owned until platform initialization makes the console available.

use riscv::register::mstatus::MPP;
use runtime::boot::{DynamicInfo, DynamicReadError, NextStage};

/// Selected next stage and raw entry address excluded from boot stacks.
pub(crate) struct Selection {
    pub(crate) stack_exclusion_entry: usize,
    pub(crate) next_stage: Result<NextStage, Error>,
}

/// A previous-stage handoff that cannot be used by this firmware build.
#[derive(Debug)]
pub(crate) enum Error {
    #[cfg(not(any(feature = "payload", feature = "jump")))]
    DynamicRead(DynamicReadError),
    #[cfg(not(any(feature = "payload", feature = "jump")))]
    InvalidDynamic {
        info: DynamicInfo,
        violation: DynamicPolicyViolation,
    },
}

/// The invalid fields reported together after complete dynamic policy checking.
#[cfg(not(any(feature = "payload", feature = "jump")))]
#[derive(Debug)]
pub(crate) enum DynamicPolicyViolation {
    Address,
    PrivilegeMode,
    AddressAndPrivilegeMode,
}

pub(crate) fn select(snapshot: Option<Result<DynamicInfo, DynamicReadError>>) -> Selection {
    #[cfg(feature = "payload")]
    let next_stage = {
        let _ = snapshot;
        Ok(NextStage {
            start_addr: runtime::boot::embedded_payload()
                .expect("BUG: payload firmware has no embedded payload")
                .start()
                .as_usize(),
            opaque: 0,
            next_mode: MPP::Supervisor,
        })
    };
    #[cfg(all(feature = "jump", not(feature = "payload")))]
    let next_stage = {
        let _ = snapshot;
        Ok(NextStage {
            start_addr: crate::cfg::JUMP_ADDRESS,
            opaque: 0,
            next_mode: MPP::Supervisor,
        })
    };
    #[cfg(not(any(feature = "payload", feature = "jump")))]
    let next_stage = snapshot
        .expect("BUG: dynamic firmware requires a Runtime handoff snapshot")
        .map_err(Error::DynamicRead)
        .and_then(validate_dynamic);

    let stack_exclusion_entry = match &next_stage {
        Ok(stage) => stage.start_addr,
        #[cfg(not(any(feature = "payload", feature = "jump")))]
        Err(Error::InvalidDynamic { info, .. }) => info.next_addr,
        #[cfg(not(any(feature = "payload", feature = "jump")))]
        Err(Error::DynamicRead(_)) => 0,
        #[cfg(any(feature = "payload", feature = "jump"))]
        Err(error) => match *error {},
    };
    Selection {
        stack_exclusion_entry,
        next_stage,
    }
}

#[cfg(not(any(feature = "payload", feature = "jump")))]
fn validate_dynamic(info: DynamicInfo) -> Result<NextStage, Error> {
    let valid_address = crate::cfg::DYNAMIC_NEXT_ADDR_RANGE
        .iter()
        .any(|range| info.next_addr >= range.start as usize && info.next_addr < range.end as usize);
    let next_mode = match info.next_mode {
        0 => Some(MPP::User),
        1 => Some(MPP::Supervisor),
        3 => Some(MPP::Machine),
        _ => None,
    };
    let violation = match (valid_address, next_mode.is_some()) {
        (false, true) => Some(DynamicPolicyViolation::Address),
        (true, false) => Some(DynamicPolicyViolation::PrivilegeMode),
        (false, false) => Some(DynamicPolicyViolation::AddressAndPrivilegeMode),
        (true, true) => None,
    };
    if let Some(violation) = violation {
        return Err(Error::InvalidDynamic { info, violation });
    }
    Ok(NextStage {
        start_addr: info.next_addr,
        opaque: 0,
        next_mode: next_mode.expect("validated next-stage privilege mode"),
    })
}
