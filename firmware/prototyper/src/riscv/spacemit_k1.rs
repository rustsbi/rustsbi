//! SpacemiT K1 / Ky X1 processor-specific initialization.
//!
//! Provides early init hooks, CCI-550 cache coherency setup, warmboot
//! (RVBADDR) configuration, and cold-boot handling for the SpacemiT K1 SoC
//! (also known as Ky X1), which uses SpacemiT X60 CPU cores.
//!
//! Reference: OpenSBI `platform/generic/spacemit/spacemit_k1.c` and
//! `platform/generic/include/spacemit/k1x/k1x_evb.h`.

use spacemit_riscv::register::{
    ml2setup,
    mraop::{self, Mraop, Operation},
    msetup,
};

use crate::riscv::current_hartid;

// ---------------------------------------------------------------------------
// Warmboot (RVBADDR) registers
// ---------------------------------------------------------------------------

const C0_RVBADDR_LO: usize = 0xd4282db0;
const C0_RVBADDR_HI: usize = 0xd4282db4;
const C1_RVBADDR_LO: usize = 0xd4282eb0;
const C1_RVBADDR_HI: usize = 0xd4282eb4;

// ---------------------------------------------------------------------------
// CCI-550 cache coherent interconnect
// ---------------------------------------------------------------------------

const CCI_550_BASE: usize = 0xd8500000;
const CCI_550_STATUS: usize = 0x000c;
const CCI_550_STATUS_CHANGE_PENDING: u32 = 1 << 0;

const CCI_550_SLAVE_IFACE0_OFFSET: usize = 0x1000;
const fn cci_slave_iface_offset(idx: usize) -> usize {
    CCI_550_SLAVE_IFACE0_OFFSET + 0x1000 * idx
}
const CCI_550_SNOOP_CTRL: usize = 0x0000;
const CCI_550_SNOOP_CTRL_ENABLE_SNOOPS: u32 = 1 << 0;
const CCI_550_SNOOP_CTRL_ENABLE_DVMS: u32 = 1 << 1;

// ---------------------------------------------------------------------------
// CPU topology
// ---------------------------------------------------------------------------

const PLATFORM_MAX_CPUS: usize = 8;
const PLATFORM_MAX_CPUS_PER_CLUSTER: usize = 4;
const fn cpu_to_cluster(cpu: usize) -> usize {
    cpu / PLATFORM_MAX_CPUS_PER_CLUSTER
}

const PLAT_CCI_CLUSTER0_IFACE_IX: usize = 0;
const PLAT_CCI_CLUSTER1_IFACE_IX: usize = 1;

/// CCI slave interface indices for each cluster.
const CCI_MAP: [usize; 2] = [PLAT_CCI_CLUSTER0_IFACE_IX, PLAT_CCI_CLUSTER1_IFACE_IX];

// ---------------------------------------------------------------------------
// Low-level helpers
// ---------------------------------------------------------------------------

/// Read a 32-bit MMIO register.
#[inline]
unsafe fn read32(addr: usize) -> u32 {
    unsafe { (addr as *const u32).read_volatile() }
}

/// Write a 32-bit MMIO register.
#[inline]
unsafe fn write32(addr: usize, val: u32) {
    unsafe { (addr as *mut u32).write_volatile(val) };
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Check if the platform model string identifies a SpacemiT K1 / Ky X1 SoC.
///
/// The model string is read from the DTB's `model` property, e.g.
/// `"spacemit k1-x orangepi-rv2 board"` or `"OrangePi RV2"`.
#[inline]
pub fn is_k1_compatible(model: &str) -> bool {
    let lower = model.to_ascii_lowercase();
    lower.contains("spacemit")
        || lower.contains("ky x1")
        || lower.contains("ky_x1")
        || lower.contains("orangepi-rv2")
        || lower.contains("orangepi_rv2")
        || lower.contains("orangepi rv2")
}

/// Check whether the device tree identifies a SpacemiT K1 / Ky X1 SoC.
///
/// The root node's `compatible` strings
/// (e.g. `"spacemit,k1"`, `"spacemit,k1-x"`) take priority,
/// with the `model` string as a fallback.
#[inline]
pub fn is_k1_platform<'a>(model: &str, compatibles: impl IntoIterator<Item = &'a str>) -> bool {
    let by_compatible = compatibles
        .into_iter()
        .any(|c| c.to_ascii_lowercase().starts_with("spacemit,k1"));
    by_compatible || is_k1_compatible(model)
}

/// Enable CCI-550 snoop and DVM messages for a given cluster.
///
/// # Safety
///
/// Must only be called once per cluster during cold boot.
unsafe fn cci_enable_snoop_dvm_reqs(cluster_id: usize) {
    let slave_if_id = CCI_MAP[cluster_id];
    let ctrl_addr = CCI_550_BASE + cci_slave_iface_offset(slave_if_id) + CCI_550_SNOOP_CTRL;

    // Enable snoops and DVM messages
    unsafe {
        write32(
            ctrl_addr,
            CCI_550_SNOOP_CTRL_ENABLE_SNOOPS | CCI_550_SNOOP_CTRL_ENABLE_DVMS,
        );
    }

    // Memory barrier before checking status
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

    // Wait for change to complete
    unsafe { while read32(CCI_550_BASE + CCI_550_STATUS) & CCI_550_STATUS_CHANGE_PENDING != 0 {} }
}

/// Pre-initialization: set warmboot addresses and enable CCI-550.
///
/// # Safety
///
/// Must only be called once during cold boot on the boot hart.
unsafe fn k1_pre_init(warmboot_addr: u64) {
    // Write warmboot address to both clusters
    unsafe {
        write32(C0_RVBADDR_LO, warmboot_addr as u32);
        write32(C0_RVBADDR_HI, (warmboot_addr >> 32) as u32);
        write32(C1_RVBADDR_LO, warmboot_addr as u32);
        write32(C1_RVBADDR_HI, (warmboot_addr >> 32) as u32);
    }

    // Enable CCI-550 snoop/DVM only for the cluster of the current (boot)
    // hart. OpenSBI likewise only enables snoop for the boot hart's own
    // cluster ("we only enable snoop of cluster0"); touching the other
    // cluster's interface while it is powered down is unsafe.
    let cluster = cpu_to_cluster(current_hartid());
    unsafe { cci_enable_snoop_dvm_reqs(cluster) };
}

/// Early initialization for the SpacemiT K1 SoC.
///
/// Should be called from `main()` on the init hart after the
/// device tree has been parsed and the K1 SoC has been detected.
///
/// # Arguments
///
/// * `cold_boot` - `true` if this is a cold boot (boot hart only).
/// * `warmboot_addr` - The warmboot entry address for secondary harts.
///
/// # Safety
///
/// Must only be called from M-mode on the boot hart (for cold_boot = true)
/// or on any hart (for cold_boot = false).
pub unsafe fn early_init(cold_boot: bool, warmboot_addr: u64) {
    // Enable D-cache, I-cache, branch prediction, prefetch, misaligned access, ECC
    let mut setup = msetup::read();
    setup.set_de(true);
    setup.set_ie(true);
    setup.set_bpe(true);
    setup.set_pfe(true);
    setup.set_mme(true);
    setup.set_ecce(true);
    // SAFETY: this function requires a K1 hart running in M-mode.
    unsafe { msetup::write(setup) };

    // Issue the cache-maintenance command used by K1 firmware.
    let mut operation = Mraop::from_bits(0);
    operation.set_operation(Operation::CleanInvalidate);
    // SAFETY: this function requires a K1 hart running in M-mode.
    unsafe { mraop::write(operation) };

    if cold_boot {
        unsafe { k1_pre_init(warmboot_addr) };
    }
}

/// Check whether a given hart is allowed to cold boot.
///
/// On the K1, only hart 0 is allowed to cold boot. All other harts
/// must use the warmboot path. This function also sets up the L2 cache
/// mask for the calling hart.
///
/// Returns `true` if the hart is allowed to cold boot.
pub fn cold_boot_allowed(hart_id: usize) -> bool {
    let core_slot = hart_id % PLATFORM_MAX_CPUS_PER_CLUSTER;
    // SAFETY: callers only invoke this K1-specific routine after identifying
    // the platform, and each hart updates its own ML2SETUP CSR.
    unsafe {
        ml2setup::set_snoop_enable(core_slot);
    }
    // Only hart 0 performs cold boot
    hart_id == 0
}

/// Runs the K1 cold-boot sequence on the boot hart: claims this hart's L2
/// setup bit (ML2SETUP), then programs MSETUP / I-cache invalidation and the
/// warmboot (RVBADDR) + CCI-550 interconnect state via [`early_init`].
///
/// Safe wrapper: the caller (the platform boot path) has already verified
/// from the device tree that this is a K1 platform, so the K1-specific
/// CSR/MMIO addresses used here are valid, and the sequence runs exactly
/// once on the M-mode boot hart.
pub fn cold_boot_init() {
    // Configure ML2SETUP for the boot hart
    cold_boot_allowed(current_hartid());
    // SAFETY: K1 platform verified by the caller; single execution on the
    // M-mode boot hart. The warmboot entry is this firmware's own link
    // address.
    unsafe {
        // Use the SBI link address as the warmboot entry
        let warmboot_addr = crate::cfg::SBI_LINK_START_ADDRESS as u64;
        early_init(true, warmboot_addr);
    }
}

/// Get the maximum number of CPUs supported by this platform.
#[inline]
pub const fn max_cpus() -> usize {
    PLATFORM_MAX_CPUS
}

/// Get the number of CPUs per cluster.
#[inline]
pub const fn cpus_per_cluster() -> usize {
    PLATFORM_MAX_CPUS_PER_CLUSTER
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topology() {
        assert_eq!(max_cpus(), 8);
        assert_eq!(cpus_per_cluster(), 4);
        assert_eq!(cpu_to_cluster(0), 0);
        assert_eq!(cpu_to_cluster(3), 0);
        assert_eq!(cpu_to_cluster(4), 1);
        assert_eq!(cpu_to_cluster(7), 1);
    }

    #[test]
    fn test_compatible() {
        assert!(is_k1_compatible("spacemit,k1"));
        assert!(is_k1_compatible("Ky X1"));
        assert!(is_k1_compatible("spacemit ky_x1"));
        // Actual model string used by the official OrangePi RV2 device tree.
        assert!(is_k1_compatible("OrangePi RV2"));
        assert!(!is_k1_compatible("sifive,fu740"));
    }

    #[test]
    fn test_platform_detection() {
        // OpenSBI spacemit_k1_match[] entries.
        assert!(is_k1_platform("", ["spacemit,k1-pro"]));
        assert!(is_k1_platform("", ["spacemit,k1x"]));
        assert!(is_k1_platform("", ["spacemit,k1-x"]));
        assert!(is_k1_platform("", ["spacemit,k1"]));
        // Official OrangePi RV2 device tree: compatible + model.
        assert!(is_k1_platform(
            "OrangePi RV2",
            ["xunlong,orangepi-rv2", "spacemit,k1"],
        ));
        // Model fallback when no compatible matches.
        assert!(is_k1_platform("OrangePi RV2", ["sifive,fu740"]));
        assert!(!is_k1_platform("Sifive FU740", ["sifive,fu740"]));
    }
}
