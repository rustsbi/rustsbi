//! K1 fixed register permissions and hart-local machine setup.
//!
//! The [vendor device tree] defines the APMU and CIU windows;
//! [OpenSBI's K1 header] identifies the CCI pages used by both clusters.
//!
//! [vendor device tree]: https://github.com/spacemit-com/linux-6.6/blob/0faf1c69bee4b0fa98940e453eeb90494effa342/arch/riscv/boot/dts/spacemit/k1-x.dtsi
//! [OpenSBI's K1 header]: https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/platform/generic/include/spacemit/k1.h

use fdt::node::FdtNode;

use crate::Result;
use crate::memory::{PhysAddr, RegRange};
use crate::soc::Soc;

/// A capability selected by a K1-compatible device-tree root.
#[derive(Clone, Copy)]
pub struct SpacemitK1Soc {
    _private: (),
}

impl Soc for SpacemitK1Soc {
    fn from_root(root: FdtNode<'_, '_>) -> Result<Option<Self>> {
        Ok(root
            .compatible()
            .is_some_and(|values| values.all().any(|value| value == "spacemit,k1x"))
            .then_some(Self { _private: () }))
    }
}

impl SpacemitK1Soc {
    /// Returns the application power-management register window.
    pub fn apmu_registers(self) -> Result<RegRange> {
        RegRange::from_start_len(PhysAddr::new(0xd428_2800), 0x400)
    }

    /// Returns the core-interface unit register window.
    pub fn ciu_registers(self) -> Result<RegRange> {
        RegRange::from_start_len(PhysAddr::new(0xd428_2c00), 0x2d0)
    }

    /// Returns the CCI control page and the two cluster-interface pages.
    pub fn cci_registers(self) -> Result<RegRange> {
        RegRange::from_start_len(PhysAddr::new(0xd850_0000), 0x3000)
    }

    /// Enables the current hart's L2 access and machine cache features.
    pub fn initialize_hart(self) {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        // SAFETY: This capability identifies K1; Runtime executes in M-mode.
        unsafe {
            arch::prepare_hart()
        };
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        unimplemented!("K1 hart setup requires a RISC-V target");
    }

    /// Starts a K1 hart released from hardware reset.
    ///
    /// # Safety
    ///
    /// Enter in M-mode with MPRV clear, after the boot hart publishes topology
    /// and services and enables cluster coherency. `initialize` must complete
    /// per-hart firmware setup before returning.
    #[cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), unsafe(naked))]
    pub unsafe extern "C" fn reset_entry(initialize: extern "C" fn()) -> ! {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        core::arch::naked_asm!(
            "mv a1, a0",
            ".option push",
            ".option norelax",
            "lla a0, {prepare}",
            ".option pop",
            "tail {entry}",
            prepare = sym arch::prepare_hart,
            entry = sym crate::boot::reset_hart_entry,
        );
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = initialize;
            unimplemented!("K1 reset entry requires a RISC-V target");
        }
    }
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod arch {
    use csr::{machine, vendor::spacemit::k1};

    // This runs before shared memory is accessible on a hardware-reset hart.
    #[unsafe(naked)]
    pub(super) unsafe extern "C" fn prepare_hart() {
        core::arch::naked_asm!(
            "csrr t0, {hart_id}",
            "andi t0, t0, {cluster_mask}",
            "li t1, {hart_enable}",
            "sll t1, t1, t0",
            "csrs {l2}, t1",
            "li t0, {features}",
            "csrs {setup}, t0",
            "ret",
            hart_id = const machine::HartId::NUMBER,
            cluster_mask = const k1::L2Setup::CLUSTER_HART_MASK,
            hart_enable = const k1::L2Setup::FIRST_HART_ENABLE,
            l2 = const k1::L2Setup::NUMBER,
            setup = const k1::MachineSetup::NUMBER,
            features = const {
                k1::DE::MASK | k1::IE::MASK | k1::BPE::MASK
                    | k1::PFE::MASK | k1::MME::MASK | k1::ECCE::MASK
            },
        )
    }
}
