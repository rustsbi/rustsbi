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

/// Resource description selected by a K1-compatible device-tree root.
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
    ///
    /// # Safety
    ///
    /// Confirm actual K1 hardware and execute in M-mode during hart startup.
    /// Own this hart's cache, predictor and ECC configuration. Keep secondary
    /// harts excluded until cluster coherency is ready for shared accesses.
    pub unsafe fn initialize_hart(self) {
        // SAFETY: The caller establishes K1 applicability and startup ownership.
        unsafe { arch::prepare_hart() };
    }

    /// Starts a K1 hart released from hardware reset.
    ///
    /// # Safety
    ///
    /// Enter in M-mode with MPRV clear, after the boot hart publishes topology
    /// and services and enables cluster coherency. `initialize` must complete
    /// per-hart firmware setup before returning.
    #[unsafe(naked)]
    pub unsafe extern "C" fn reset_entry(initialize: unsafe extern "C" fn()) -> ! {
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
    }
}

mod arch {
    use riscv_csr_spec::{standard::isa::privileged::machine::Mhartid, vendor::spacemit::k1};

    /// Enables K1 hart-local cache access without using shared memory.
    ///
    /// # Safety
    ///
    /// Run in M-mode on a K1 during startup. The caller must own this hart's
    /// cache, predictor and ECC configuration.
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
            hart_id = const Mhartid::NUMBER,
            cluster_mask = const k1::Ml2setup::CLUSTER_HART_MASK,
            hart_enable = const k1::Ml2setup::FIRST_HART_ENABLE,
            l2 = const k1::Ml2setup::NUMBER,
            setup = const k1::Msetup::NUMBER,
            features = const {
                k1::Msetup::DE.mask()
                    | k1::Msetup::IE.mask()
                    | k1::Msetup::BPE.mask()
                    | k1::Msetup::PFE.mask()
                    | k1::Msetup::MME.mask()
                    | k1::Msetup::ECCE.mask()
            },
        )
    }
}
