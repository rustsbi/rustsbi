//! Arm CoreLink CCI-550 coherency control.
//!
//! Platform code supplies the status and selected slave-interface windows.
//! Register fields and sequencing follow the [Arm CoreLink CCI-550 TRM],
//! sections 3.2, 3.3.3, and 3.3.10.
//!
//! [Arm CoreLink CCI-550 TRM]: https://documentation-service.arm.com/static/5e7dd450cbfe76649ba52b0c

use bitflags::bitflags;

use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

bitflags! {
    struct Status: u32 {
        const CHANGE_PENDING = 1 << 0;
    }

    struct SnoopControl: u32 {
        const ENABLE_SNOOPS = 1 << 0;
        const ENABLE_DVM_MESSAGES = 1 << 1;
    }
}

struct StatusRegister(MmioRegion);

impl StatusRegister {
    fn change_pending(&self) -> bool {
        Status::from_bits_retain(
            self.0
                .read(0)
                .expect("BUG: CCI-550 status register escaped its MMIO window"),
        )
        .contains(Status::CHANGE_PENDING)
    }
}

struct SnoopControlRegister(MmioRegion);

impl SnoopControlRegister {
    fn enable_coherency(&self) {
        self.0
            .write(
                0,
                (SnoopControl::ENABLE_SNOOPS | SnoopControl::ENABLE_DVM_MESSAGES).bits(),
            )
            .expect("BUG: CCI-550 snoop-control register escaped its MMIO window");
    }
}

/// CCI-550 coherency control for the slave interfaces selected by the platform.
pub(crate) struct Cci550<const N: usize> {
    status: StatusRegister,
    snoop_control: [SnoopControlRegister; N],
}

impl<const N: usize> Cci550<N> {
    pub(crate) fn acquire(
        memory: &mut MemoryRegistry,
        status: DeviceRegisterRange,
        snoop_controls: [DeviceRegisterRange; N],
    ) -> runtime::Result<Self> {
        let status = StatusRegister(memory.acquire_mmio(status)?);
        let mut snoop_control = [const { None }; N];
        for (control, registers) in snoop_control.iter_mut().zip(snoop_controls) {
            *control = Some(SnoopControlRegister(memory.acquire_mmio(registers)?));
        }
        Ok(Self {
            status,
            snoop_control: snoop_control
                .map(|control| control.expect("BUG: CCI-550 interface window was not acquired")),
        })
    }

    pub(crate) fn enable_coherency(&self) {
        for control in &self.snoop_control {
            control.enable_coherency();
            // Order the MMIO write before polling status; an atomic fence
            // only orders ordinary memory, not device I/O.
            riscv::asm::fence();
            while self.status.change_pending() {
                core::hint::spin_loop();
            }
        }
    }
}
