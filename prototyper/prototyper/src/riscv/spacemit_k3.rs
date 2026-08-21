//! SpacemiT K3 processor-specific initialization.
//!
//! Provides early init hooks, CCI-550 cache coherency setup, warmboot
//! (RVBADDR) configuration, and cold-boot handling for the SpacemiT K3 SoC,
//! which uses 8× SpacemiT X100 CPU cores (two 4-core clusters).
//!
//! Register definitions and initialization flow mirror the official OpenSBI
//! implementation: `platform/generic/spacemit/spacemit_k3.c`,
//! `platform/generic/include/spacemit/k3/{k3.h,core_common.h}` and
//! `lib/utils/cci/bus-cci-550.c`. MMIO registers are accessed through typed
//! layouts (`MmioReg`, [`WarmbootAddr`], [`Cci550Registers`]) rather than raw
//! base-address arithmetic, keeping the register layouts testable via
//! `offset_of!`.

use core::arch::asm;

// ---------------------------------------------------------------------------
// Custom CSRs (0x7f0, 0x7f7)
// ---------------------------------------------------------------------------

/// Machine L2 cache setup register.
const CSR_ML2SETUP: u16 = 0x7f0;
/// Machine L2 cache hint register.
const CSR_ML2HINT: u16 = 0x7f7;

// ML2SETUP bit fields (in addition to the per-hart L2 cache mask)
const ML2SETUP_IPRF: usize = 1 << 16; // Instruction prefetch enable
const ML2SETUP_TPRF: usize = 1 << 18; // TLB prefetch enable

// ML2HINT bit fields
const ML2HINT_TRACE_TOP_ICGEN: usize = 1 << 26; // RV-Trace top clock enable

// ---------------------------------------------------------------------------
// Typed MMIO helpers
// ---------------------------------------------------------------------------

/// A 32-bit volatile MMIO register.
#[repr(transparent)]
struct MmioReg(u32);

impl MmioReg {
    /// Read the register.
    #[inline]
    fn read(&self) -> u32 {
        // Safety: `self` points at MMIO registers; reads are volatile so the
        // compiler cannot cache or reorder them.
        unsafe { core::ptr::addr_of!(self.0).read_volatile() }
    }

    /// Write the register.
    #[inline]
    fn write(&self, val: u32) {
        // Safety: `self` points at MMIO registers; writes are volatile so
        // the compiler cannot elide or reorder them.
        unsafe { core::ptr::addr_of!(self.0).cast_mut().write_volatile(val) }
    }
}

// ---------------------------------------------------------------------------
// Warmboot (RVBADDR) registers
// ---------------------------------------------------------------------------

/// A pair of adjacent LO/HI warmboot address registers (8 bytes).
#[repr(C)]
struct WarmbootAddr {
    lo: MmioReg,
    hi: MmioReg,
}

impl WarmbootAddr {
    /// Write the 64-bit warmboot entry address (LO then HI).
    #[inline]
    fn set(&self, addr: u64) {
        self.lo.write(addr as u32);
        self.hi.write((addr >> 32) as u32);
    }
}

/// Cluster 0 warmboot address (OpenSBI k3.h `C0_RVBADDR_LO/HI_ADDR`).
const C0_RVBADDR: *const WarmbootAddr = 0xd4282db0usize as *const WarmbootAddr;
/// Cluster 1 warmboot address (OpenSBI k3.h `C1_RVBADDR_LO/HI_ADDR`).
const C1_RVBADDR: *const WarmbootAddr = (0xd4282c00usize + 0x2b0) as *const WarmbootAddr;

// ---------------------------------------------------------------------------
// PMU L2 cache flush control
// ---------------------------------------------------------------------------

/// Cluster 0 L2 flush control register (OpenSBI k3.h `PMU_C0_L2_FLUSH_CTRL`).
const PMU_C0_L2_FLUSH_CTRL: *const MmioReg = (0xd8440000usize + 0x1b0) as *const MmioReg;
/// Cluster 1 L2 flush control register (OpenSBI k3.h `PMU_C1_L2_FLUSH_CTRL`).
const PMU_C1_L2_FLUSH_CTRL: *const MmioReg = (0xd8440000usize + 0x1b4) as *const MmioReg;
const PMU_L2_FLUSH_HW_TYPE: u32 = 1 << 0; // Hardware flush type
const PMU_L2_FLUSH_HW_EN: u32 = 1 << 2; // Hardware flush enable

// ---------------------------------------------------------------------------
// CCI-550 cache coherent interconnect
// ---------------------------------------------------------------------------

/// CCI-550 control/status registers (bus-cci-550.c `CTRL_OVERRIDE`/`STATUS`).
#[repr(C)]
struct Cci550Registers {
    _ctrl_override: MmioReg, // 0x0000
    _reserved0: MmioReg,     // 0x0004
    _reserved1: MmioReg,     // 0x0008
    status: MmioReg,         // 0x000c
}

impl Cci550Registers {
    /// Whether a snoop control change is still pending (bus-cci-550.c
    /// `CHANGE_PENDING_BIT`).
    #[inline]
    fn change_pending(&self) -> bool {
        self.status.read() & CCI_550_STATUS_CHANGE_PENDING != 0
    }

    /// Registers of the slave interface at `idx` (bus-cci-550.c
    /// `SLAVE_IFACE_OFFSET(index) = 0x1000 + 0x1000 * index`).
    ///
    /// # Safety
    ///
    /// `idx` must be a valid CCI-550 slave interface index (0..7).
    #[inline]
    unsafe fn slave_iface(&self, idx: usize) -> &CciSlaveIfaceRegisters {
        // Safety: `self` is a valid CCI-550 base address and `idx` is a valid
        // slave interface index, so the computed address aliases its MMIO.
        unsafe {
            &*((self as *const Self as usize + cci_slave_iface_offset(idx))
                as *const CciSlaveIfaceRegisters)
        }
    }
}

/// CCI-550 slave interface registers (bus-cci-550.c `SNOOP_CTRL_REG`).
#[repr(C)]
struct CciSlaveIfaceRegisters {
    snoop_ctrl: MmioReg, // 0x0000
}

const CCI_550_BASE: *const Cci550Registers = 0xd8500000usize as *const Cci550Registers;
const CCI_550_STATUS_CHANGE_PENDING: u32 = 1 << 0;

const CCI_550_SLAVE_IFACE0_OFFSET: usize = 0x1000;
const fn cci_slave_iface_offset(idx: usize) -> usize {
    CCI_550_SLAVE_IFACE0_OFFSET + 0x1000 * idx
}
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
const PLAT_CCI_CLUSTER2_IFACE_IX: usize = 2;
const PLAT_CCI_CLUSTER3_IFACE_IX: usize = 3;

/// CCI slave interface indices for each cluster (OpenSBI k3.h `PLAT_CCI_MAP`).
const CCI_MAP: [usize; 4] = [
    PLAT_CCI_CLUSTER0_IFACE_IX,
    PLAT_CCI_CLUSTER1_IFACE_IX,
    PLAT_CCI_CLUSTER2_IFACE_IX,
    PLAT_CCI_CLUSTER3_IFACE_IX,
];

// ---------------------------------------------------------------------------
// Low-level helpers
// ---------------------------------------------------------------------------

/// Read a CSR.
///
/// `CSR` must be a compile-time constant: the RISC-V `csrr` encoding requires
/// the CSR field to be an immediate, not a register.
#[inline]
unsafe fn csr_read<const CSR: u16>() -> usize {
    let r: usize;
    unsafe {
        asm!("csrr {r}, {csr}", r = out(reg) r, csr = const CSR, options(nomem));
    }
    r
}

/// Write a CSR.
///
/// `CSR` must be a compile-time constant: the RISC-V `csrw` encoding requires
/// the CSR field to be an immediate, not a register.
#[inline]
unsafe fn csr_write<const CSR: u16>(val: usize) {
    unsafe {
        asm!("csrw {csr}, {val}", csr = const CSR, val = in(reg) val, options(nomem));
    }
}

/// Set bits in a CSR (read-modify-write).
///
/// `CSR` must be a compile-time constant; see [`csr_write`].
#[inline]
unsafe fn csr_set<const CSR: u16>(bits: usize) {
    let old = unsafe { csr_read::<CSR>() };
    unsafe { csr_write::<CSR>(old | bits) };
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Check if the platform model string identifies a SpacemiT K3 SoC.
///
/// The model string is read from the DTB's `model` property, e.g.
/// `"SpacemiT K3 Pico-ITX"`, `"SpacemiT K3 CoM260 Module"`,
/// `"SpacemiT K3 CoM260 IFX"` or `"DeepComputing FML13V05"`.
///
/// Unlike the K1 detection, a bare `"spacemit"` match is deliberately NOT
/// used here: it would also match K1 boards, whose model strings share the
/// `"SpacemiT ..."` prefix. Only K3-specific markers are accepted.
#[inline]
pub fn is_k3_compatible(model: &str) -> bool {
    let lower = model.to_ascii_lowercase();
    lower.contains("k3")
        || lower.contains("pico-itx")
        || lower.contains("pico_itx")
        || lower.contains("com260")
        || lower.contains("fml13v05")
}

/// Check whether the device tree identifies a SpacemiT K3 SoC.
///
/// Mirrors OpenSBI's `spacemit_k3_mach[]` table (`"spacemit,k3"` and
/// `"riscv-spacemit"`): the root node's `compatible` strings take priority,
/// with the `model` string as a fallback.
#[inline]
pub fn is_k3_platform<'a>(model: &str, compatibles: impl IntoIterator<Item = &'a str>) -> bool {
    let by_compatible = compatibles.into_iter().any(|c| {
        c.to_ascii_lowercase().starts_with("spacemit,k3")
            || c.eq_ignore_ascii_case("riscv-spacemit")
    });
    by_compatible || is_k3_compatible(model)
}

/// Enable CCI-550 snoop and DVM messages for a given cluster.
///
/// Mirrors OpenSBI `cci_enable_snoop_dvm_reqs()` (bus-cci-550.c): write the
/// snoop control register, then wait for the change to complete.
///
/// # Safety
///
/// Must only be called once per cluster during cold boot.
unsafe fn cci_enable_snoop_dvm_reqs(cluster_id: usize) {
    let slave_if_id = CCI_MAP[cluster_id];
    let cci = unsafe { &*CCI_550_BASE };
    let iface = unsafe { cci.slave_iface(slave_if_id) };

    // Enable snoops and DVM messages (no RMW: other bits are write-ignore)
    iface
        .snoop_ctrl
        .write(CCI_550_SNOOP_CTRL_ENABLE_SNOOPS | CCI_550_SNOOP_CTRL_ENABLE_DVMS);

    // Memory barrier before checking status
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

    // Wait for change to complete
    unsafe { while cci.change_pending() {} }
}

/// Pre-initialization: set warmboot addresses and enable CCI-550.
///
/// # Safety
///
/// Must only be called once during cold boot on the boot hart.
unsafe fn k3_pre_init(warmboot_addr: u64) {
    // Write the warmboot address to each cluster and flush its L2 cache with
    // the hardware flush engine, mirroring OpenSBI's cold-boot loop.
    for cluster in 0..PLATFORM_MAX_CPUS / PLATFORM_MAX_CPUS_PER_CLUSTER {
        let (rvbaddr, l2_flush_ctrl) = match cluster {
            0 => (C0_RVBADDR, PMU_C0_L2_FLUSH_CTRL),
            1 => (C1_RVBADDR, PMU_C1_L2_FLUSH_CTRL),
            // The 8-core K3 has two 4-core clusters; the C2/C3 warmboot and
            // flush registers exist for 16-core parts (OpenSBI k3.h) and are
            // left untouched.
            _ => unreachable!(),
        };
        unsafe {
            (*rvbaddr).set(warmboot_addr);
            // Hardware L2 cache flush (OpenSBI: PMU_L2_FLUSH_HW_EN | PMU_L2_FLUSH_HW_TYPE)
            (*l2_flush_ctrl).write(PMU_L2_FLUSH_HW_EN | PMU_L2_FLUSH_HW_TYPE);
        }
    }

    // Enable CCI-550 snoop/DVM on every cluster interface. OpenSBI calls
    // cci_enable_snoop_dvm_reqs(0..6) — its cci_slave_if_map[] only has 4
    // entries (one per cluster), so the effective range is interfaces 0..3,
    // which is exactly what iterating CCI_MAP covers.
    for cluster in 0..CCI_MAP.len() {
        unsafe { cci_enable_snoop_dvm_reqs(cluster) };
    }
}

/// Early initialization for the SpacemiT K3 SoC.
///
/// Should be called from `main()` on the init hart after the
/// device tree has been parsed and the K3 SoC has been detected.
///
/// # Arguments
///
/// * `cold_boot` - `true` if this is a cold boot (boot hart only).
/// * `warmboot_addr` - The warmboot entry address for secondary harts.
///
/// # Safety
///
/// Must only be called from M-mode on the boot hart (for cold_boot = true).
pub unsafe fn early_init(cold_boot: bool, warmboot_addr: u64) {
    if cold_boot {
        unsafe { k3_pre_init(warmboot_addr) };
    }
    // On the X100, cache/branch-prediction setup is handled per-hart in
    // [`cold_boot_allowed`] (ML2SETUP), matching OpenSBI: the X100 boots
    // with caches enabled, so no MSETUP/MRAOP sequence is required here.
}

/// Check whether a given hart is allowed to cold boot.
///
/// On the K3, only hart 0 is allowed to cold boot. All other harts
/// must use the warmboot path. This function also enables core snoop,
/// instruction prefetch and TLB prefetch for the calling hart, and turns
/// on the RV-Trace top clock.
///
/// Returns `true` if the hart is allowed to cold boot.
pub fn cold_boot_allowed(hart_id: usize) -> bool {
    // Set the ML2SETUP bit for this hart's position in its cluster, plus
    // IPRF/TPRF (OpenSBI spacemit_k3_cold_boot_allowed). OpenSBI additionally
    // tunes VEC_L1BYPASS / L2 prefetch distance / ML2HINT correlation checks
    // for hartid >= 8, which never applies to the 8-core K3.
    let cluster_bit = 1 << (hart_id % PLATFORM_MAX_CPUS_PER_CLUSTER);
    unsafe {
        csr_set::<CSR_ML2SETUP>(cluster_bit | ML2SETUP_IPRF | ML2SETUP_TPRF);
        // Enable the RV-Trace top clock by default.
        csr_set::<CSR_ML2HINT>(ML2HINT_TRACE_TOP_ICGEN);
    }
    // Only hart 0 performs cold boot
    hart_id == 0
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
    use core::mem::offset_of;

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
        // Actual model strings used by the official K3 board device trees.
        assert!(is_k3_compatible("SpacemiT K3 Pico-ITX"));
        assert!(is_k3_compatible("SpacemiT K3 CoM260 Module"));
        assert!(is_k3_compatible("SpacemiT K3 CoM260 IFX"));
        assert!(is_k3_compatible("DeepComputing FML13V05"));
        assert!(!is_k3_compatible("OrangePi RV2"));
        assert!(!is_k3_compatible("sifive,fu740"));
    }

    #[test]
    fn test_platform_detection() {
        // OpenSBI spacemit_k3_mach[] entries.
        assert!(is_k3_platform("", ["spacemit,k3"]));
        assert!(is_k3_platform("", ["riscv-spacemit"]));
        // Official K3 board device trees: compatible + model.
        assert!(is_k3_platform("", ["spacemit,k3-pico-itx", "spacemit,k3"]));
        assert!(is_k3_platform("", ["spacemit,k3-com260", "spacemit,k3"]));
        assert!(is_k3_platform(
            "",
            ["spacemit,k3-com260-ifx", "spacemit,k3"]
        ));
        assert!(is_k3_platform(
            "",
            ["deepcomputing,fml13v05", "spacemit,k3"]
        ));
        // Model fallback when no compatible matches.
        assert!(is_k3_platform("SpacemiT K3 Pico-ITX", ["sifive,fu740"]));
        assert!(!is_k3_platform("OrangePi RV2", ["spacemit,k1"]));
        assert!(!is_k3_platform("Sifive FU740", ["sifive,fu740"]));
    }

    #[test]
    fn test_register_layout() {
        // MMIO accessor widths (32-bit registers).
        assert_eq!(core::mem::size_of::<MmioReg>(), 4);
        // Warmboot address: adjacent LO/HI 32-bit registers.
        assert_eq!(offset_of!(WarmbootAddr, lo), 0x0);
        assert_eq!(offset_of!(WarmbootAddr, hi), 0x4);
        assert_eq!(core::mem::size_of::<WarmbootAddr>(), 8);
        // CCI-550 status register sits at 0x000c (bus-cci-550.c STATUS_REG).
        assert_eq!(offset_of!(Cci550Registers, status), 0x000c);
        // CCI-550 slave interface snoop control at offset 0 (SNOOP_CTRL_REG).
        assert_eq!(offset_of!(CciSlaveIfaceRegisters, snoop_ctrl), 0x0);
        // SLAVE_IFACE_OFFSET(index) = 0x1000 + 0x1000 * index.
        assert_eq!(cci_slave_iface_offset(0), 0x1000);
        assert_eq!(cci_slave_iface_offset(3), 0x4000);
    }
}
