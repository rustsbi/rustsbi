//! Arm CoreLink CCI-550 coherency control.
//!
//! Platform code supplies the status and selected slave-interface windows.
//! Register fields and sequencing follow the [Arm CoreLink CCI-550 TRM],
//! sections 3.2, 3.3.3, and 3.3.10.
//!
//! [Arm CoreLink CCI-550 TRM]: https://documentation-service.arm.com/static/5e7dd450cbfe76649ba52b0c

use bitflags::bitflags;
use runtime::{
    Result,
    memory::{MemoryRegistry, MmioRegion},
};

use super::description::Desc;

bitflags! {
    /// Status bits for the CCI-550 status register.
    struct Status: u32 {
        const CHANGE_PENDING = 1 << 0;
    }

    /// Bits that enable CCI-550 snooping and DVM messages.
    struct SnoopControl: u32 {
        const ENABLE_SNOOPS = 1 << 0;
        const ENABLE_DVM_MESSAGES = 1 << 1;
    }
}

/// An acquired CCI-550 status-register window.
struct StatusRegister(MmioRegion);

impl StatusRegister {
    /// Returns whether a coherency change is still pending.
    fn change_pending(&self) -> Result<bool> {
        Ok(Status::from_bits_retain(self.0.read(0)?).contains(Status::CHANGE_PENDING))
    }
}

/// An acquired CCI-550 snoop-control window.
struct SnoopControlRegister(MmioRegion);

impl SnoopControlRegister {
    /// Enables snoops and DVM messages on this slave interface.
    fn enable_coherency(&self) -> Result<()> {
        self.0.write(
            0,
            (SnoopControl::ENABLE_SNOOPS | SnoopControl::ENABLE_DVM_MESSAGES).bits(),
        )
    }
}

/// CCI-550 coherency control for the slave interfaces selected by the platform.
pub(crate) struct Cci550<const N: usize> {
    status: StatusRegister,
    snoop_control: [SnoopControlRegister; N],
}

impl<const N: usize> Cci550<N> {
    /// Acquires the described register windows and constructs the controller.
    pub(crate) fn init(description: Desc<N>, memory: &mut MemoryRegistry) -> Result<Self> {
        let status = StatusRegister(memory.acquire(description.status)?);
        let snoop_control = core::array::try_from_fn(|index| {
            memory
                .acquire(description.snoop_controls[index])
                .map(SnoopControlRegister)
        })?;
        Ok(Self {
            status,
            snoop_control,
        })
    }

    /// Enables coherency on every selected interface and waits for each change to clear.
    pub(crate) fn enable_coherency(&self) -> Result<()> {
        for control in &self.snoop_control {
            control.enable_coherency()?;
            // Order the preceding MMIO write before the status poll.
            runtime::instruction::fence();
            while self.status.change_pending()? {
                core::hint::spin_loop();
            }
        }
        Ok(())
    }
}
