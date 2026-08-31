//! SpacemiT K3 processor-specific initialization.
//!
//! Provides early init hooks, CCI-550 cache coherency setup, warmboot
//! (RVBADDR) configuration, A100 cluster parking, power-domain voting and
//! cold-boot handling for the SpacemiT K3 SoC (8× X100 RVA23 cores + 8×
//! A100 AI cores).
//!
//! MMIO registers are accessed through typed layouts using
//! `volatile_register::{RW, RO, WO}` rather than raw base-address arithmetic,
//! keeping access permissions explicit and register layouts testable via
//! `offset_of!`.

use core::arch::asm;
use volatile_register::{RO, RW, WO};

// ---------------------------------------------------------------------------
// Custom CSRs (0x7f0, 0x7f7, 0x7d0, 0x7d1)
// ---------------------------------------------------------------------------

/// Machine L2 cache setup register.
const CSR_ML2SETUP: u16 = 0x7f0;
/// Machine L2 cache hint register.
const CSR_ML2HINT: u16 = 0x7f7;
/// Machine performance control register (vector load cache level).
const CSR_PERF_CTRL: u16 = 0x7d0;
/// Machine prefetch control register (L2 prefetch distance).
const CSR_PREFETCH_CTRL: u16 = 0x7d1;

// ML2SETUP bit fields (in addition to the per-hart L2 cache mask)
const ML2SETUP_IPRF: usize = 1 << 16; // Instruction prefetch enable
const ML2SETUP_TPRF: usize = 1 << 18; // TLB prefetch enable

// ML2HINT bit fields (core_common.h)
const ML2HINT_CIU_CHR2_MER_DIS: usize = 1 << 2; // Disable read/prefetch transaction merge
const ML2HINT_CIU_CHR2_DEPD_DIS: usize = 1 << 3; // Disable full address dependency check
const ML2HINT_TRACE_TOP_ICGEN: usize = 1 << 26; // RV-Trace top clock enable

// PERF_CTRL bit fields (core_common.h)
const PERF_CTRL_VEC_L1BYPASS: usize = 1 << 32; // Vector loads bypass L1, cached in L2 only

// PREFETCH_CTRL bit fields (core_common.h)
const PREFETCH_CTRL_L2_PERF_DIST: usize = 3 << 10; // L2 prefetch distance: 56 entries

// ---------------------------------------------------------------------------
// Warmboot (RVBADDR) registers
// ---------------------------------------------------------------------------

/// A pair of adjacent LO/HI warmboot address registers (8 bytes).
#[repr(C)]
struct WarmbootAddr {
    lo: WO<u32>,
    hi: WO<u32>,
}

impl WarmbootAddr {
    /// Write the 64-bit warmboot entry address (LO then HI).
    #[inline]
    unsafe fn set(&self, addr: u64) {
        unsafe {
            self.lo.write(addr as u32);
            self.hi.write((addr >> 32) as u32);
        }
    }
}

/// Cluster 0 warmboot entry address (LO/HI register pair).
const C0_RVBADDR: *const WarmbootAddr = 0xd4282db0usize as *const WarmbootAddr;
/// Cluster 1 warmboot entry address (LO/HI register pair).
const C1_RVBADDR: *const WarmbootAddr = (0xd4282c00usize + 0x2b0) as *const WarmbootAddr;
/// Cluster 2 warmboot entry address (LO/HI register pair).
const C2_RVBADDR: *const WarmbootAddr = (0xd4282c00usize + 0x3e8) as *const WarmbootAddr;
/// Cluster 3 warmboot entry address (LO/HI register pair).
const C3_RVBADDR: *const WarmbootAddr = (0xd4282c00usize + 0x260) as *const WarmbootAddr;

// ---------------------------------------------------------------------------
// PMU registers (k3.h)
// ---------------------------------------------------------------------------

/// Base of the PMU capability register block.
const PMU_CAP_BASE: usize = 0xd4282800;

/// `PMU_CAP_CORE*_WAKEUP` register addresses, indexed by hart ID
/// (k3.h L32-47).
const PMU_CAP_CORE_WAKEUP: [*const WO<u32>; 16] = [
    (PMU_CAP_BASE + 0x12c) as *const WO<u32>, // CORE0
    (PMU_CAP_BASE + 0x130) as *const WO<u32>, // CORE1
    (PMU_CAP_BASE + 0x134) as *const WO<u32>, // CORE2
    (PMU_CAP_BASE + 0x138) as *const WO<u32>, // CORE3
    (PMU_CAP_BASE + 0x324) as *const WO<u32>, // CORE4
    (PMU_CAP_BASE + 0x328) as *const WO<u32>, // CORE5
    (PMU_CAP_BASE + 0x32c) as *const WO<u32>, // CORE6
    (PMU_CAP_BASE + 0x330) as *const WO<u32>, // CORE7
    (PMU_CAP_BASE + 0x360) as *const WO<u32>, // CORE8
    (PMU_CAP_BASE + 0x364) as *const WO<u32>, // CORE9
    (PMU_CAP_BASE + 0x368) as *const WO<u32>, // CORE10
    (PMU_CAP_BASE + 0x36c) as *const WO<u32>, // CORE11
    (PMU_CAP_BASE + 0x22c) as *const WO<u32>, // CORE12
    (PMU_CAP_BASE + 0x230) as *const WO<u32>, // CORE13
    (PMU_CAP_BASE + 0x234) as *const WO<u32>, // CORE14
    (PMU_CAP_BASE + 0x238) as *const WO<u32>, // CORE15
];

/// `PMU_CAP_CORE*_IDLE_CFG` register addresses, indexed by hart ID
/// (k3.h L49-64).
const PMU_CAP_CORE_IDLE_CFG: [*const RW<u32>; 16] = [
    (PMU_CAP_BASE + 0x124) as *const RW<u32>, // CORE0
    (PMU_CAP_BASE + 0x128) as *const RW<u32>, // CORE1
    (PMU_CAP_BASE + 0x160) as *const RW<u32>, // CORE2
    (PMU_CAP_BASE + 0x164) as *const RW<u32>, // CORE3
    (PMU_CAP_BASE + 0x304) as *const RW<u32>, // CORE4
    (PMU_CAP_BASE + 0x308) as *const RW<u32>, // CORE5
    (PMU_CAP_BASE + 0x30c) as *const RW<u32>, // CORE6
    (PMU_CAP_BASE + 0x310) as *const RW<u32>, // CORE7
    (PMU_CAP_BASE + 0x340) as *const RW<u32>, // CORE8
    (PMU_CAP_BASE + 0x344) as *const RW<u32>, // CORE9
    (PMU_CAP_BASE + 0x348) as *const RW<u32>, // CORE10
    (PMU_CAP_BASE + 0x34c) as *const RW<u32>, // CORE11
    (PMU_CAP_BASE + 0x20c) as *const RW<u32>, // CORE12
    (PMU_CAP_BASE + 0x210) as *const RW<u32>, // CORE13
    (PMU_CAP_BASE + 0x214) as *const RW<u32>, // CORE14
    (PMU_CAP_BASE + 0x218) as *const RW<u32>, // CORE15
];

/// `PMU_CX_CAPMP_IDLE_CFG*` cluster power/idle config registers, indexed by
/// hart ID (k3.h L66-81).
const PMU_CX_CAPMP_IDLE_CFG: [*const RW<u32>; 16] = [
    (PMU_CAP_BASE + 0x120) as *const RW<u32>, // CFG0  (cluster 0, hart 0)
    (PMU_CAP_BASE + 0xe4) as *const RW<u32>,  // CFG1  (cluster 0, hart 1)
    (PMU_CAP_BASE + 0x150) as *const RW<u32>, // CFG2  (cluster 0, hart 2)
    (PMU_CAP_BASE + 0x154) as *const RW<u32>, // CFG3  (cluster 0, hart 3)
    (PMU_CAP_BASE + 0x314) as *const RW<u32>, // CFG4  (cluster 1, hart 4)
    (PMU_CAP_BASE + 0x318) as *const RW<u32>, // CFG5  (cluster 1, hart 5)
    (PMU_CAP_BASE + 0x31c) as *const RW<u32>, // CFG6  (cluster 1, hart 6)
    (PMU_CAP_BASE + 0x320) as *const RW<u32>, // CFG7  (cluster 1, hart 7)
    (PMU_CAP_BASE + 0x350) as *const RW<u32>, // CFG8  (cluster 2, hart 8)
    (PMU_CAP_BASE + 0x354) as *const RW<u32>, // CFG9  (cluster 2, hart 9)
    (PMU_CAP_BASE + 0x358) as *const RW<u32>, // CFG10 (cluster 2, hart 10)
    (PMU_CAP_BASE + 0x35c) as *const RW<u32>, // CFG11 (cluster 2, hart 11)
    (PMU_CAP_BASE + 0x21c) as *const RW<u32>, // CFG12 (cluster 3, hart 12)
    (PMU_CAP_BASE + 0x220) as *const RW<u32>, // CFG13 (cluster 3, hart 13)
    (PMU_CAP_BASE + 0x224) as *const RW<u32>, // CFG14 (cluster 3, hart 14)
    (PMU_CAP_BASE + 0x228) as *const RW<u32>, // CFG15 (cluster 3, hart 15)
];

// Power/idle config bit fields (k3.h)
const CPU_MASK_FI_INTTERUPT: u32 = (1 << 3) | (1 << 4);
const CPU_PWR_DOWN_VALUE: u32 = 0x1f;
const CLUSTER_PWR_DOWN_VALUE: u32 = 0x8f;

/// `APCR_CORE*_VETE_REG` register addresses, indexed by hart ID (k3.h L83-98).
const APCR_CORE_VETE_REG: [*const WO<u32>; 16] = [
    (0xd4050000usize + 0x10c0) as *const WO<u32>, // CORE0
    (0xd4050000usize + 0x10c4) as *const WO<u32>, // CORE1
    (0xd4050000usize + 0x10c8) as *const WO<u32>, // CORE2
    (0xd4050000usize + 0x10cc) as *const WO<u32>, // CORE3
    (0xd4050000usize + 0x10d0) as *const WO<u32>, // CORE4
    (0xd4050000usize + 0x10d4) as *const WO<u32>, // CORE5
    (0xd4050000usize + 0x10d8) as *const WO<u32>, // CORE6
    (0xd4050000usize + 0x10dc) as *const WO<u32>, // CORE7
    (0xd4050000usize + 0x10e0) as *const WO<u32>, // CORE8
    (0xd4050000usize + 0x10e4) as *const WO<u32>, // CORE9
    (0xd4050000usize + 0x10e8) as *const WO<u32>, // CORE10
    (0xd4050000usize + 0x10ec) as *const WO<u32>, // CORE11
    (0xd4050000usize + 0x10f0) as *const WO<u32>, // CORE12
    (0xd4050000usize + 0x10f4) as *const WO<u32>, // CORE13
    (0xd4050000usize + 0x10f8) as *const WO<u32>, // CORE14
    (0xd4050000usize + 0x10fc) as *const WO<u32>, // CORE15
];

/// Default APCR VATE value written by `spacemit_vote_core_apcr` (k3.h L101).
const APCR_COREX_DEFAULT_VATE_VALUE: u32 = (1 << 3)
    | (1 << 13)
    | (1 << 14)
    | (1 << 19)
    | (1 << 25)
    | (1 << 26)
    | (1 << 27)
    | (1 << 29)
    | (1 << 31);

// ---------------------------------------------------------------------------
// PMU L2 cache flush control (k3.h)
// ---------------------------------------------------------------------------

/// PMU L2 flush control register base (per-cluster offsets).
const PMU_L2_FLUSH_BASE: usize = 0xd8440000;
const PMU_C0_L2_FLUSH_CTRL: *const WO<u32> = (PMU_L2_FLUSH_BASE + 0x1b0) as *const WO<u32>;
const PMU_C1_L2_FLUSH_CTRL: *const WO<u32> = (PMU_L2_FLUSH_BASE + 0x1b4) as *const WO<u32>;
const PMU_C2_L2_FLUSH_CTRL: *const WO<u32> = (PMU_L2_FLUSH_BASE + 0x1c4) as *const WO<u32>;
const PMU_C3_L2_FLUSH_CTRL: *const WO<u32> = (PMU_L2_FLUSH_BASE + 0x1ec) as *const WO<u32>;
const PMU_L2_FLUSH_HW_TYPE: u32 = 1 << 0; // Hardware flush type
const PMU_L2_FLUSH_HW_EN: u32 = 1 << 2; // Hardware flush enable

/// DMASYS reset/clock registers (k3.h L121-122).
const DMASYS_RESET: *const WO<u32> = (PMU_L2_FLUSH_BASE + 0x22c) as *const WO<u32>;
const DMASYS_CLK_EN: *const WO<u32> = (PMU_L2_FLUSH_BASE + 0x234) as *const WO<u32>;

// ---------------------------------------------------------------------------
// CCI-550 cache coherent interconnect
// ---------------------------------------------------------------------------

/// CCI-550 control/status registers (bus-cci-550.c `CTRL_OVERRIDE`/`STATUS`).
#[repr(C)]
struct Cci550Registers {
    _ctrl_override: RW<u32>, // 0x0000
    _reserved: u32,          // 0x0004
    _secure_access: RW<u32>, // 0x0008
    status: RO<u32>,         // 0x000c
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
    snoop_ctrl: RW<u32>, // 0x0000
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

/// Clear bits in a CSR (read-modify-write).
///
/// `CSR` must be a compile-time constant; see [`csr_write`].
#[inline]
unsafe fn csr_clear<const CSR: u16>(bits: usize) {
    let old = unsafe { csr_read::<CSR>() };
    unsafe { csr_write::<CSR>(old & !bits) };
}

// ---------------------------------------------------------------------------
// Power-domain voting (k3_corepm.c)
// ---------------------------------------------------------------------------

/// Vote a core's power-down (`spacemit_vote_powrdown_core`, k3_corepm.c
/// L387-476): set `CPU_PWR_DOWN_VALUE` in the core's `PMU_CAP_CORE*_IDLE_CFG`.
fn vote_powrdown_core(hartid: usize) {
    let reg = unsafe { &*PMU_CAP_CORE_IDLE_CFG[hartid] };
    unsafe { reg.modify(|value| value | CPU_PWR_DOWN_VALUE) };
}

/// Vote a core and its cluster power-down (`spacemit_vote_powrdown_cluster`,
/// k3_corepm.c L478-615): additionally set `CLUSTER_PWR_DOWN_VALUE` in the
/// cluster's `PMU_CX_CAPMP_IDLE_CFG*`.
fn vote_powrdown_cluster(hartid: usize) {
    let core_reg = unsafe { &*PMU_CAP_CORE_IDLE_CFG[hartid] };
    let cluster_reg = unsafe { &*PMU_CX_CAPMP_IDLE_CFG[hartid] };
    unsafe {
        core_reg.modify(|value| value | CPU_PWR_DOWN_VALUE);
        cluster_reg.modify(|value| value | CLUSTER_PWR_DOWN_VALUE);
    }
}

/// Cancel a core's and cluster's power-down votes
/// (`spacemit_devote_pwrdown_cluster`, k3_corepm.c L805-943): clear the
/// power-down and interrupt-mask bits, so the cluster stays powered for boot.
fn devote_pwrdown_cluster(hartid: usize) {
    let core_reg = unsafe { &*PMU_CAP_CORE_IDLE_CFG[hartid] };
    let cluster_reg = unsafe { &*PMU_CX_CAPMP_IDLE_CFG[hartid] };
    unsafe {
        core_reg.modify(|value| value & !(CPU_PWR_DOWN_VALUE | CPU_MASK_FI_INTTERUPT));
        cluster_reg.modify(|value| value & !CLUSTER_PWR_DOWN_VALUE);
    }
}

/// Vote a core's APCR VETE register with the default value
/// (`spacemit_vote_core_apcr`, k3_corepm.c L114-203).
fn vote_core_apcr(hartid: usize) {
    let reg = unsafe { &*APCR_CORE_VETE_REG[hartid] };
    unsafe { reg.write(APCR_COREX_DEFAULT_VATE_VALUE) };
}

/// Cancel a core's APCR VETE vote (`spacemit_devote_core_apcr`, k3_corepm.c
/// L205-294).
fn devote_core_apcr(hartid: usize) {
    let reg = unsafe { &*APCR_CORE_VETE_REG[hartid] };
    unsafe { reg.write(0) };
}

/// Wake up a core by asserting its `PMU_CAP_CORE*_WAKEUP` register
/// (`spacemit_wakeup_core`, k3_corepm.c L1057-1113).
///
/// Writing the wakeup register is benign for a core that is already running
/// (it only matters while the core is in a PMU low-power state), so it can be
/// asserted unconditionally before raising the software IPI in HSM
/// `hart_start`.
pub(crate) fn wakeup_core(hartid: usize) {
    let reg = unsafe { &*PMU_CAP_CORE_WAKEUP[hartid] };
    unsafe { reg.write(1 << hartid) };
}

// ---------------------------------------------------------------------------
// A100 parking entry (spacemit_k3.c `boot_entry_dummy`, L51-92)
// ---------------------------------------------------------------------------

/// Parking entry for the first A100 core (hart 8, cluster 2).
///
/// The boot hart points `C2_RVBADDR` at this function and asserts
/// `PMU_CAP_CORE8_WAKEUP`, waking core8 which then performs the vector-load
/// tuning, votes its cluster/core down, disables caches and snoop, and hangs
/// in a `wfi` loop.
///
/// This function never returns.
fn boot_entry_dummy() -> ! {
    let hartid = crate::riscv::current_hartid();

    unsafe {
        // Vector-load tuning (spacemit_k3.c L53-59)
        csr_set::<CSR_PERF_CTRL>(PERF_CTRL_VEC_L1BYPASS);
        csr_set::<CSR_PREFETCH_CTRL>(PREFETCH_CTRL_L2_PERF_DIST);
        csr_clear::<CSR_ML2HINT>(ML2HINT_CIU_CHR2_DEPD_DIS);
        csr_set::<CSR_ML2HINT>(ML2HINT_CIU_CHR2_MER_DIS);

        // Devote early (spacemit_k3.c L61-65)
        devote_pwrdown_cluster(hartid);
        devote_core_apcr(hartid);

        // Re-set the boot entry of cluster2 to the normal warm entry
        // (spacemit_k3.c L67-69): a later HSM start of an A100 core would
        // otherwise land back on this dummy. The warm entry is the same
        // `_start_warm_k3` used for the X100 clusters.
        (*C2_RVBADDR).set(warm_entry());

        // Vote core8 power-down (spacemit_k3.c L71)
        vote_powrdown_core(8);

        // Disable local timer (spacemit_k3.c L73-74)
        csr_write::<0x14d>(usize::MAX); // CSR_STIMECMP

        // Disable all IRQs (spacemit_k3.c L75-76)
        const MIP_ALL: usize = (1 << 1) | (1 << 3) | (1 << 5) | (1 << 7) | (1 << 9) | (1 << 11);
        csr_clear::<0x304>(MIP_ALL); // CSR_MIE

        // Disable prefetch, flush dcache, and disable caches before
        // parking; the trailing fence orders the change for other masters.
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

        // Disable core snoop (spacemit_k3.c L86-87)
        csr_clear::<CSR_ML2SETUP>(1 << (hartid % PLATFORM_MAX_CPUS_PER_CLUSTER));
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }

    loop {
        unsafe { asm!("wfi", options(nomem, nostack)) };
    }
}

// ---------------------------------------------------------------------------
// CCI-550 snoop enable
// ---------------------------------------------------------------------------

/// Enable CCI-550 snoop and DVM messages on a slave interface.
///
/// Write the snoop control register, then wait for the change to complete.
///
/// # Safety
///
/// Must only be called once per interface during cold boot. `slave_if_id`
/// must be a valid CCI-550 slave interface index (0..6).
unsafe fn cci_enable_snoop_dvm_reqs(slave_if_id: usize) {
    let cci = unsafe { &*CCI_550_BASE };
    let iface = unsafe { cci.slave_iface(slave_if_id) };

    // Enable snoops and DVM messages (no RMW: other bits are write-ignore)
    unsafe {
        iface
            .snoop_ctrl
            .write(CCI_550_SNOOP_CTRL_ENABLE_SNOOPS | CCI_550_SNOOP_CTRL_ENABLE_DVMS);
    }

    // Memory barrier before checking status
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

    // Wait for change to complete
    while cci.change_pending() {}
}

// ---------------------------------------------------------------------------
// Pre-initialization (spacemit_k3.c `spacemit_k3_early_init`, L96-204)
// ---------------------------------------------------------------------------

/// Pre-initialization: warmboot addresses, CCI-550, A100 parking, DMASYS.
///
/// # Safety
///
/// Must only be called once during cold boot on the boot hart.
unsafe fn k3_pre_init(warmboot_addr: u64) {
    // Write the warmboot entry to every cluster and flush each cluster's L2
    // cache with the hardware flush engine (spacemit_k3.c L107-153). The two
    // X100 clusters (0, 1) get the SBI warm entry; the two A100 clusters
    // (2, 3) get the parking entry so any A100 core that wakes parks safely
    // instead of entering the SBI (which only manages the 8 X100 harts).
    for cluster in 0..4 {
        let (rvbaddr, l2_flush_ctrl) = match cluster {
            0 => (C0_RVBADDR, PMU_C0_L2_FLUSH_CTRL),
            1 => (C1_RVBADDR, PMU_C1_L2_FLUSH_CTRL),
            2 => (C2_RVBADDR, PMU_C2_L2_FLUSH_CTRL),
            3 => (C3_RVBADDR, PMU_C3_L2_FLUSH_CTRL),
            _ => unreachable!(),
        };
        // Every cluster (X100 and A100) gets the warm entry, so all A100
        // harts come online through HSM hart_start like the X100 harts
        // instead of being parked.
        let entry = warmboot_addr;
        unsafe {
            (*rvbaddr).set(entry);
            // Hardware L2 cache flush (k3.h PMU_L2_FLUSH_HW_EN | HW_TYPE)
            (*l2_flush_ctrl).write(PMU_L2_FLUSH_HW_EN | PMU_L2_FLUSH_HW_TYPE);
        }
    }

    // Enable CCI-550 snoop/DVM on all seven slave interfaces (0..=6).
    for slave_if_id in 0..=6 {
        unsafe { cci_enable_snoop_dvm_reqs(slave_if_id) };
    }

    // Devote (un-vote) every hart's cluster so it stays powered (spacemit_k3.c
    // iterates all platform harts), then wake core8 -?the first A100 core of
    // cluster2 -?into the parking entry (spacemit_k3.c L164-166). The dummy
    // entry re-points C2 RVBADDR at the warm entry before parking core8, so
    // later HSM starts of harts 8-11 land on the warm entry.
    for hartid in 0..PLATFORM_MAX_CPUS {
        devote_pwrdown_cluster(hartid);
    }
    unsafe {
        (*C2_RVBADDR).set(boot_entry_dummy as *const () as usize as u64);
    }
    wakeup_core(8);

    // Deassert DMASYS reset and enable its clock so CPUs can reach the full
    // TCM range (spacemit_k3.c L170-174).
    unsafe {
        (*DMASYS_RESET).write(1);
        (*DMASYS_CLK_EN).write(1);
    }
}

/// Early initialization for the SpacemiT K3 SoC.
///
/// Should be called from `init_board()` on the boot hart after the device
/// tree has been parsed and the K3 SoC has been detected.
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
    if cold_boot {
        unsafe { k3_pre_init(warmboot_addr) };
    }
    // Per-hart cache/branch-prediction setup is handled in
    // [`cold_boot_allowed`] (ML2SETUP).
}

/// Performs K3 initialization on the boot hart.
pub fn cold_boot_init() {
    unsafe { early_init(true, warm_entry()) }
}

/// Check whether a given hart is allowed to cold boot.
///
/// On the K3, only hart 0 is allowed to cold boot; all other harts must use
/// the warmboot path. This function also enables core snoop, instruction
/// prefetch and TLB prefetch for the calling hart, applies the A100
/// vector-load tuning for `hartid -?8`, and turns on the RV-Trace top clock
/// (spacemit_k3.c `spacemit_k3_cold_boot_allowed`, L217-246).
///
/// Returns `true` if the hart is allowed to cold boot.
pub fn cold_boot_allowed(hart_id: usize) -> bool {
    let cluster_bit = 1 << (hart_id % PLATFORM_MAX_CPUS_PER_CLUSTER);
    unsafe {
        csr_set::<CSR_ML2SETUP>(cluster_bit | ML2SETUP_IPRF | ML2SETUP_TPRF);

        if hart_id >= 8 {
            // A100 vector-load tuning (spacemit_k3.c L222-230)
            csr_set::<CSR_PERF_CTRL>(PERF_CTRL_VEC_L1BYPASS);
            csr_set::<CSR_PREFETCH_CTRL>(PREFETCH_CTRL_L2_PERF_DIST);
            csr_clear::<CSR_ML2HINT>(ML2HINT_CIU_CHR2_DEPD_DIS);
            csr_set::<CSR_ML2HINT>(ML2HINT_CIU_CHR2_MER_DIS);
        }

        // Enable the RV-Trace top clock by default (spacemit_k3.c L233)
        csr_set::<CSR_ML2HINT>(ML2HINT_TRACE_TOP_ICGEN);

        // Devote early (spacemit_k3.c L236-239)
        devote_pwrdown_cluster(hart_id);
        devote_core_apcr(hart_id);
    }
    // Only hart 0 performs cold boot
    hart_id == 0
}

// The common entry macro exports this bridge for platform-specific startup
// paths. K3's warm entry invokes it after completing its SoC initialization.
unsafe extern "C" {
    #[link_name = "__rustsbi_prototyper_main"]
    fn prototyper_main(_a0: usize, a1: usize, a2: usize);

    #[link_name = "_start_warm_k3"]
    static START_WARM_K3: u8;
}

/// SpacemiT K3 warm-boot entry, referenced by the cluster RVBADDR registers
/// after platform initialization.
#[doc(hidden)]
#[unsafe(naked)]
#[unsafe(link_section = ".text.warmboot")]
#[unsafe(export_name = "_start_warm_k3")]
unsafe extern "C" fn start_warm_k3() -> ! {
    core::arch::naked_asm!(
        include_str!("../entry/spacemit_k3.S"),
        locate_stack = sym crate::sbi::trap_stack::locate,
        main = sym prototyper_main,
        hart_boot = sym crate::sbi::trap::boot::boot,
    )
}

/// Address of the K3 warm-boot entry (`_start_warm_k3` in spacemit_k3.S).
///
/// PMU-woken secondary harts fetch their cluster RVBADDR, which the boot
/// hart points at this entry.
pub fn warm_entry() -> u64 {
    core::ptr::addr_of!(START_WARM_K3) as u64
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

// ---------------------------------------------------------------------------
// Platform detection (spacemit_k3.c `spacemit_k3_mach[]`, L40-44)
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
/// The root node's `compatible` strings (`"spacemit,k3"`,
/// `"riscv-spacemit"`) take priority, with the `model` string as a fallback.
#[inline]
pub fn is_k3_platform<'a>(model: &str, compatibles: impl IntoIterator<Item = &'a str>) -> bool {
    let by_compatible = compatibles.into_iter().any(|c| {
        c.to_ascii_lowercase().starts_with("spacemit,k3")
            || c.eq_ignore_ascii_case("riscv-spacemit")
    });
    by_compatible || is_k3_compatible(model)
}

// ---------------------------------------------------------------------------
// RCPU runtime memory regions (k3.h L108-119)
// ---------------------------------------------------------------------------

/// RCPU0 runtime memory region base/size (k3.h L108-110).
pub const RCPU0_RUNTIME_SPACE_BASE_ADDR: usize = 0x100200000;
pub const RCPU0_RUNTIME_SPACE_SIZE: usize = 0x400000;
/// RCPU1 runtime memory region base/size (k3.h L111-113).
pub const RCPU1_RUNTIME_SPACE_BASE_ADDR: usize = 0x100800000;
pub const RCPU1_RUNTIME_SPACE_SIZE: usize = 0x400000;
/// RCPU DTB memory region base/size (k3.h L115-117).
pub const RCPU_DTB_SPACE_BASE_ADDR: usize = 0x100d00000;
pub const RCPU_DTB_SPACE_SIZE: usize = 0x100000;

/// REGISTER_PRESERVATION region: S-mode accesses are emulated by M-mode
/// (spacemit_k3.c `REGISTER_PRESERVATION_*`, k3.h L114-115).
pub const REGISTER_PRESERVATION_BASE: usize = 0xd4282000;
pub const REGISTER_PRESERVATION_SIZE: usize = 0x1000;

// ---------------------------------------------------------------------------
// REGISTER_PRESERVATION S-mode load/store emulation (spacemit_k3.c
// `s_addr_to_pa`, `m_only_ranges`, `emulate_load`, `emulate_store`, L254-374)
// ---------------------------------------------------------------------------

/* Sv39 page-table walk constants (spacemit_k3.c L28-38) */
const SATP64_MODE_SHIFT: usize = 60;
const SV39_LEVELS: usize = 3;
const SV39_VPN_BITS: usize = 9;
const SV39_VPN_MASK: usize = (1 << SV39_VPN_BITS) - 1;
const SV39_VPN2_SHIFT: usize = 30;
const SV39_VPN1_SHIFT: usize = 21;
const SV39_VPN0_SHIFT: usize = 12; // PAGE_SHIFT
const PTE_V: usize = 1 << 0; // valid
const PTE_RWX: usize = 0xe; // leaf: R|W|X any set
const PTE_PPN_SHIFT: usize = 10;

/// Translate an S-mode virtual address to a physical address.
///
/// If S-mode MMU is off (`satp.MODE == Bare`), `addr` is already physical.
/// Otherwise walk the Sv39 page tables, which M-mode can read directly.
/// Returns `None` on translation failure (spacemit_k3.c `s_addr_to_pa`,
/// L254-289).
pub fn s_addr_to_pa(addr: usize) -> Option<usize> {
    unsafe {
        let satp: usize;
        core::arch::asm!("csrr {}, satp", out(reg) satp, options(nomem));
        let mode = (satp >> SATP64_MODE_SHIFT) & 0xf;

        // Bare mode: no translation, addr is already physical
        if mode == 0 {
            return Some(addr);
        }
        if mode != 8 {
            // Only Sv39 is handled (spacemit_k3.c returns 0 otherwise)
            return None;
        }

        let mut ppn = satp & ((1 << 44) - 1); // SATP64_PPN (44 bits)
        let vpn = [
            (addr >> SV39_VPN2_SHIFT) & SV39_VPN_MASK,
            (addr >> SV39_VPN1_SHIFT) & SV39_VPN_MASK,
            (addr >> SV39_VPN0_SHIFT) & SV39_VPN_MASK,
        ];

        for (i, &vpn_i) in vpn.iter().enumerate() {
            let ptep = (ppn << 12) + vpn_i * core::mem::size_of::<usize>();
            let pte = *(ptep as *const usize);

            if pte & PTE_V == 0 {
                return None; // invalid PTE
            }

            ppn = (pte >> PTE_PPN_SHIFT) & ((1 << 44) - 1);

            if pte & PTE_RWX != 0 {
                // leaf PTE: R|W|X set
                let pg_off_bits = 12 + SV39_VPN_BITS * (2 - i);
                let offset_mask = (1usize << pg_off_bits) - 1;
                return Some((ppn << 12) | (addr & offset_mask));
            }
        }
        None
    }
}

/// Local representation of one range in the K3-specific M-mode-only register
/// table. S-mode accesses to these ranges return `None` so the fault is
/// redirected back to S-mode as a real access error (spacemit_k3.c
/// `m_only_ranges[]`, L304-326).
struct AddrRange {
    base: usize,
    size: usize,
}

const fn addr_range(base: usize, size: usize) -> AddrRange {
    AddrRange { base, size }
}

const M_ONLY_RANGES: [AddrRange; 11] = [
    addr_range(0xd4282db0, 2 * 4),          // C0_RVBADDR LO/HI
    addr_range(0xd4282c00 + 0x2b0, 2 * 4),  // C1_RVBADDR LO/HI
    addr_range(0xd4282c00 + 0x3e8, 2 * 4),  // C2_RVBADDR LO/HI
    addr_range(0xd4282c00 + 0x260, 2 * 4),  // C3_RVBADDR LO/HI
    addr_range(0xd4282800 + 0xe4, 4),       // PMU_CX_CAPMP_IDLE_CFG1
    addr_range(0xd4282800 + 0x120, 7 * 4),  // CFG0, IDLE_CFG0/1, WAKEUP0-3
    addr_range(0xd4282800 + 0x150, 2 * 4),  // CFG2/3
    addr_range(0xd4282800 + 0x160, 2 * 4),  // IDLE_CFG2/3
    addr_range(0xd4282800 + 0x20c, 12 * 4), // IDLE_CFG12-15, CX_CFG12-15, WAKEUP12-15
    addr_range(0xd4282800 + 0x304, 12 * 4), // IDLE_CFG4-7, CX_CFG4-7, WAKEUP4-7
    addr_range(0xd4282800 + 0x340, 12 * 4), // IDLE_CFG8-11, CX_CFG8-11, WAKEUP8-11
];

fn pa_is_m_only(pa: usize, len: usize) -> bool {
    let Some(end) = pa.checked_add(len) else {
        return false;
    };

    M_ONLY_RANGES.iter().any(|r| {
        r.base
            .checked_add(r.size)
            .is_some_and(|range_end| pa >= r.base && end <= range_end)
    })
}

/// Emulate an S-mode load from the REGISTER_PRESERVATION window.
///
/// Translate the faulting virtual address, refuse M-mode-only registers, and
/// read the physical register. Returns `None` when the access must be
/// reported back to S-mode as a fault.
pub fn emulate_load(addr: usize, len: usize) -> Option<u64> {
    let pa = s_addr_to_pa(addr)?;

    let Some(end) = pa.checked_add(len) else {
        return None;
    };
    let Some(register_end) = REGISTER_PRESERVATION_BASE.checked_add(REGISTER_PRESERVATION_SIZE)
    else {
        return None;
    };
    if pa < REGISTER_PRESERVATION_BASE || end > register_end {
        return None;
    }

    // M-mode-only registers: refuse S-mode emulation
    if pa_is_m_only(pa, len) {
        return None;
    }

    // Safety: `pa` is within the verified REGISTER_PRESERVATION window.
    Some(match len {
        1 => unsafe { (pa as *const u8).read_volatile() as u64 },
        2 => unsafe { (pa as *const u16).read_volatile() as u64 },
        4 => unsafe { (pa as *const u32).read_volatile() as u64 },
        8 => unsafe { (pa as *const u64).read_volatile() },
        _ => return None,
    })
}

/// Emulate an S-mode store to the REGISTER_PRESERVATION window.
///
/// Returns `false` when the access must be reported back to S-mode as a
/// fault.
pub fn emulate_store(addr: usize, len: usize, val: u64) -> bool {
    let pa = match s_addr_to_pa(addr) {
        Some(pa) => pa,
        None => return false,
    };

    let Some(end) = pa.checked_add(len) else {
        return false;
    };
    let Some(register_end) = REGISTER_PRESERVATION_BASE.checked_add(REGISTER_PRESERVATION_SIZE)
    else {
        return false;
    };
    if pa < REGISTER_PRESERVATION_BASE || end > register_end {
        return false;
    }

    // M-mode-only registers: refuse S-mode emulation
    if pa_is_m_only(pa, len) {
        return false;
    }

    // Safety: `pa` is within the verified REGISTER_PRESERVATION window.
    match len {
        1 => unsafe { (pa as *mut u8).write_volatile(val as u8) },
        2 => unsafe { (pa as *mut u16).write_volatile(val as u16) },
        4 => unsafe { (pa as *mut u32).write_volatile(val as u32) },
        8 => unsafe { (pa as *mut u64).write_volatile(val) },
        _ => return false,
    }
    true
}

// ---------------------------------------------------------------------------
// IMSIC interrupt state save/restore (k3_corepm.c `__rpmi_hsm_suspend_pre`,
// `__rpmi_hsm_suspend`, `__rpmi_hsm_resume`, L621-1021; k3.h L148-186)
// ---------------------------------------------------------------------------

/* IMSIC indirect CSR selectors (k3.h L149-153) */
const IMSIC_EIDELIVERY: usize = 0x70;
const IMSIC_EITHRESHOLD: usize = 0x72;
const IMSIC_FIRST_EIE_REG: usize = 0xc0;
const MAX_IMSIC_EIE_REGISTERS: usize = 64;
const IMSIC_MAX_VGEN: usize = 0x8;

/* TOPEI interrupt-ID shift (the top bits carry the pending interrupt ID) */
const TOPEI_ID_SHIFT: usize = 16;

/* Indirect CSR numbers (RISC-V AIA) */
const CSR_MISELECT: usize = 0x350;
const CSR_MIREG: usize = 0x351;
const CSR_SISELECT: usize = 0x150;
const CSR_SIREG: usize = 0x151;
const CSR_VSISELECT: usize = 0x250;
const CSR_VSIREG: usize = 0x251;

/* Machine interrupt enable bits (MIP_* in mie) */
const MIP_ALL: usize = (1 << 1) | (1 << 3) | (1 << 5) | (1 << 7) | (1 << 9) | (1 << 11);

/// IMSIC interrupt state saved across suspend (k3.h `struct imsic_config`,
/// L162-186). The M- and S-mode levels are saved for every hart; the H/VS
/// levels only for X100 harts (`hartid < 8`), which implement the hypervisor
/// extension -?the A100 cores do not (official paper §4.1).
#[derive(Clone, Copy, Default)]
pub struct ImsicConfig {
    /* m-mode */
    pub meidelivery: usize,
    pub meithreshold: usize,
    pub meie: [usize; MAX_IMSIC_EIE_REGISTERS / 2],
    /* s-mode */
    pub seidelivery: usize,
    pub seithreshold: usize,
    pub seie: [usize; MAX_IMSIC_EIE_REGISTERS / 2],
    /* h-mode (hartid < 8 only) */
    pub hstatus: usize,
    pub hedeleg: usize,
    pub hideleg: usize,
    pub hie: usize,
    pub hcounteren: usize,
    pub hgeie: usize,
    pub henvcfg: usize,
    pub htval: usize,
    pub hgatp: usize,
    pub htimedelta: usize,
    /* vs-level (hartid < 8 only) */
    pub hc: [HimsicConfig; IMSIC_MAX_VGEN],
}

/// VS-level IMSIC state per virtual guest (k3.h `struct himsic_config`,
/// L155-160).
#[derive(Clone, Copy, Default)]
pub struct HimsicConfig {
    pub heidelivery: usize,
    pub heithreshold: usize,
    pub heie: [usize; MAX_IMSIC_EIE_REGISTERS / 2],
}

/// Reads an indirect CSR (write `select` to the *ISELECT CSR, read the
/// *IREG CSR), restoring the previous select afterwards.
///
/// The `*ISELECT` and `*IREG` CSRs are encoded as immediates in the
/// instruction stream, so both must be compile-time constants.
///
/// # Safety
///
/// The current hart must implement Smaia and be permitted to access the
/// corresponding privilege-level CSRs.
#[inline]
unsafe fn indirect_read<const SELECT: u16, const REG: u16>(reg_id: usize) -> usize {
    unsafe {
        // Swap the previous *ISELECT value out into `prev`.
        let prev: usize;
        core::arch::asm!(
            "csrrw {prev}, {sel}, {id}",
            prev = out(reg) prev,
            sel = const SELECT,
            id = in(reg) reg_id,
            options(nomem),
        );
        let value: usize;
        core::arch::asm!(
            "csrr {val}, {reg}",
            val = out(reg) value,
            reg = const REG,
            options(nomem),
        );
        // Restore the previous *ISELECT value.
        core::arch::asm!(
            "csrw {sel}, {prev}",
            sel = const SELECT,
            prev = in(reg) prev,
            options(nomem),
        );
        value
    }
}

/// Writes an indirect CSR (write `select` to the *ISELECT CSR, write the
/// *IREG CSR), restoring the previous select afterwards.
///
/// The `*ISELECT` and `*IREG` CSRs are encoded as immediates in the
/// instruction stream, so both must be compile-time constants.
///
/// # Safety
///
/// The current hart must implement Smaia and be permitted to access the
/// corresponding privilege-level CSRs.
#[inline]
unsafe fn indirect_write<const SELECT: u16, const REG: u16>(reg_id: usize, value: usize) {
    unsafe {
        // Swap the previous *ISELECT value out into `prev`.
        let prev: usize;
        core::arch::asm!(
            "csrrw {prev}, {sel}, {id}",
            prev = out(reg) prev,
            sel = const SELECT,
            id = in(reg) reg_id,
            options(nomem),
        );
        core::arch::asm!(
            "csrw {reg}, {val}",
            reg = const REG,
            val = in(reg) value,
            options(nomem),
        );
        // Restore the previous *ISELECT value.
        core::arch::asm!(
            "csrw {sel}, {prev}",
            sel = const SELECT,
            prev = in(reg) prev,
            options(nomem),
        );
    }
}

/// Saves the M-/S-mode IMSIC interrupt-file state of the current hart
/// (k3_corepm.c `__rpmi_hsm_suspend` L698-720).
///
/// # Safety
///
/// The current hart must implement Smaia.
pub unsafe fn imsic_save_machine_supervisor(state: &mut ImsicConfig) {
    unsafe {
        /* save m-mode */
        state.meithreshold =
            indirect_read::<{ CSR_MISELECT as u16 }, { CSR_MIREG as u16 }>(IMSIC_EITHRESHOLD);
        state.meidelivery =
            indirect_read::<{ CSR_MISELECT as u16 }, { CSR_MIREG as u16 }>(IMSIC_EIDELIVERY);
        for (i, sel) in (0..MAX_IMSIC_EIE_REGISTERS).step_by(2).enumerate() {
            state.meie[i] = indirect_read::<{ CSR_MISELECT as u16 }, { CSR_MIREG as u16 }>(
                IMSIC_FIRST_EIE_REG + sel,
            );
        }

        /* save s-mode */
        state.seithreshold =
            indirect_read::<{ CSR_SISELECT as u16 }, { CSR_SIREG as u16 }>(IMSIC_EITHRESHOLD);
        state.seidelivery =
            indirect_read::<{ CSR_SISELECT as u16 }, { CSR_SIREG as u16 }>(IMSIC_EIDELIVERY);
        for (i, sel) in (0..MAX_IMSIC_EIE_REGISTERS).step_by(2).enumerate() {
            state.seie[i] = indirect_read::<{ CSR_SISELECT as u16 }, { CSR_SIREG as u16 }>(
                IMSIC_FIRST_EIE_REG + sel,
            );
        }
    }
}

/// Restores the M-/S-mode IMSIC interrupt-file state of the current hart
/// (k3_corepm.c `__rpmi_hsm_resume` L960-982).
///
/// # Safety
///
/// The current hart must implement Smaia.
pub unsafe fn imsic_restore_machine_supervisor(state: &ImsicConfig) {
    unsafe {
        /* restore m-mode */
        indirect_write::<{ CSR_MISELECT as u16 }, { CSR_MIREG as u16 }>(
            IMSIC_EITHRESHOLD,
            state.meithreshold,
        );
        indirect_write::<{ CSR_MISELECT as u16 }, { CSR_MIREG as u16 }>(
            IMSIC_EIDELIVERY,
            state.meidelivery,
        );
        for (i, sel) in (0..MAX_IMSIC_EIE_REGISTERS).step_by(2).enumerate() {
            indirect_write::<{ CSR_MISELECT as u16 }, { CSR_MIREG as u16 }>(
                IMSIC_FIRST_EIE_REG + sel,
                state.meie[i],
            );
        }

        /* restore s-mode */
        indirect_write::<{ CSR_SISELECT as u16 }, { CSR_SIREG as u16 }>(
            IMSIC_EITHRESHOLD,
            state.seithreshold,
        );
        indirect_write::<{ CSR_SISELECT as u16 }, { CSR_SIREG as u16 }>(
            IMSIC_EIDELIVERY,
            state.seidelivery,
        );
        for (i, sel) in (0..MAX_IMSIC_EIE_REGISTERS).step_by(2).enumerate() {
            indirect_write::<{ CSR_SISELECT as u16 }, { CSR_SIREG as u16 }>(
                IMSIC_FIRST_EIE_REG + sel,
                state.seie[i],
            );
        }
    }
}

/// Saves the H/VS-mode IMSIC state of the current hart (k3_corepm.c
/// `__rpmi_hsm_suspend` L722-758). Only meaningful on X100 harts
/// (`hartid < 8`) that implement the hypervisor extension.
///
/// # Safety
///
/// The current hart must implement Smaia and the hypervisor extension.
pub unsafe fn imsic_save_hypervisor(state: &mut ImsicConfig) {
    unsafe {
        /* hs level */
        state.hstatus = csr_read_raw::<0x600>(); // CSR_HSTATUS
        state.hedeleg = csr_read_raw::<0x602>(); // CSR_HEDELEG
        state.hideleg = csr_read_raw::<0x603>(); // CSR_HIDELEG
        state.hie = csr_read_raw::<0x604>(); // CSR_HIE
        state.hcounteren = csr_read_raw::<0x606>(); // CSR_HCOUNTEREN
        state.hgeie = csr_read_raw::<0x607>(); // CSR_HGEIE
        state.henvcfg = csr_read_raw::<0x60a>(); // CSR_HENVCFG
        state.htval = csr_read_raw::<0x643>(); // CSR_HTVAL
        state.hgatp = csr_read_raw::<0x680>(); // CSR_HGATP
        state.htimedelta = csr_read_raw::<0x605>(); // CSR_HTIMEDELTA

        /* vs level: iterate VGEIN 1..8, saving each guest's file */
        for k in 1..IMSIC_MAX_VGEN {
            let s = (state.hstatus & !(0x3f << 12)) | (k << 12);
            csr_write_raw::<0x600>(s); // set HSTATUS.VGEIN = k

            state.hc[k].heithreshold =
                indirect_read::<{ CSR_VSISELECT as u16 }, { CSR_VSIREG as u16 }>(IMSIC_EITHRESHOLD);
            state.hc[k].heidelivery =
                indirect_read::<{ CSR_VSISELECT as u16 }, { CSR_VSIREG as u16 }>(IMSIC_EIDELIVERY);
            for (i, sel) in (0..MAX_IMSIC_EIE_REGISTERS).step_by(2).enumerate() {
                state.hc[k].heie[i] = indirect_read::<
                    { CSR_VSISELECT as u16 },
                    { CSR_VSIREG as u16 },
                >(IMSIC_FIRST_EIE_REG + sel);
            }
        }
    }
}

/// Restores the H/VS-mode IMSIC state of the current hart (k3_corepm.c
/// `__rpmi_hsm_resume` L984-1020). See [`imsic_save_hypervisor`].
///
/// # Safety
///
/// The current hart must implement Smaia and the hypervisor extension.
pub unsafe fn imsic_restore_hypervisor(state: &ImsicConfig) {
    unsafe {
        /* vs level: restore each guest's file first (VGEIN is still k) */
        for k in 1..IMSIC_MAX_VGEN {
            let s = (state.hstatus & !(0x3f << 12)) | (k << 12);
            csr_write_raw::<0x600>(s);

            indirect_write::<{ CSR_VSISELECT as u16 }, { CSR_VSIREG as u16 }>(
                IMSIC_EITHRESHOLD,
                state.hc[k].heithreshold,
            );
            indirect_write::<{ CSR_VSISELECT as u16 }, { CSR_VSIREG as u16 }>(
                IMSIC_EIDELIVERY,
                state.hc[k].heidelivery,
            );
            for (i, sel) in (0..MAX_IMSIC_EIE_REGISTERS).step_by(2).enumerate() {
                indirect_write::<{ CSR_VSISELECT as u16 }, { CSR_VSIREG as u16 }>(
                    IMSIC_FIRST_EIE_REG + sel,
                    state.hc[k].heie[i],
                );
            }
        }

        /* hs level */
        csr_write_raw::<0x602>(state.hedeleg);
        csr_write_raw::<0x603>(state.hideleg);
        csr_write_raw::<0x604>(state.hie);
        csr_write_raw::<0x606>(state.hcounteren);
        csr_write_raw::<0x607>(state.hgeie);
        csr_write_raw::<0x60a>(state.henvcfg);
        csr_write_raw::<0x643>(state.htval);
        csr_write_raw::<0x680>(state.hgatp);
        csr_write_raw::<0x605>(state.htimedelta);
        csr_write_raw::<0x600>(state.hstatus);
    }
}

/// Reads an arbitrary CSR by number (for the hypervisor state fields).
///
/// The CSR is encoded as an immediate in the instruction stream, so it must
/// be a compile-time constant.
///
/// # Safety
///
/// The CSR must be implemented on the current hart.
#[inline]
unsafe fn csr_read_raw<const CSR: u16>() -> usize {
    let value: usize;
    unsafe {
        core::arch::asm!("csrr {val}, {csr}", val = out(reg) value, csr = const CSR, options(nomem));
    }
    value
}

/// Writes an arbitrary CSR by number.
///
/// The CSR is encoded as an immediate in the instruction stream, so it must
/// be a compile-time constant.
///
/// # Safety
///
/// The CSR must be implemented on the current hart.
#[inline]
unsafe fn csr_write_raw<const CSR: u16>(value: usize) {
    unsafe {
        core::arch::asm!("csrw {csr}, {val}", csr = const CSR, val = in(reg) value, options(nomem));
    }
}

/// Masks the fast-interrupt mask bits for the current hart
/// (`spacemit_mask_irq`, k3_corepm.c L296-385): set `CPU_MASK_FI_INTTERUPT`
/// in the hart's `PMU_CAP_CORE*_IDLE_CFG`.
fn mask_irq(hartid: usize) {
    let reg = unsafe { &*PMU_CAP_CORE_IDLE_CFG[hartid] };
    unsafe { reg.modify(|value| value | CPU_MASK_FI_INTTERUPT) };
}

/// Unmasks the fast-interrupt mask bits for the current hart
/// (`spacemit_unmask_irq`, k3_corepm.c L296-385).
fn unmask_irq(hartid: usize) {
    let reg = unsafe { &*PMU_CAP_CORE_IDLE_CFG[hartid] };
    unsafe { reg.modify(|value| value & !CPU_MASK_FI_INTTERUPT) };
}

/// Suspend preparation for the current hart (`__rpmi_hsm_suspend_pre`,
/// k3_corepm.c L621-683): mask fast interrupts, then verify no M/S/VS
/// interrupt is pending via the IMSIC TOPEI registers. The check must pass
/// five consecutive clean rounds -?any pending interrupt aborts immediately
/// and restores the masks; otherwise the core and cluster 3 APCR votes are
/// placed.
///
/// Returns `true` if the hart may proceed with suspend.
pub fn suspend_pre(hartid: usize) -> bool {
    mask_irq(hartid);

    let mut retry_count = 5usize;

    while retry_count != 0 {
        retry_count -= 1;

        // 1. query s-mode irq pending (CSR_STOPEI, 0x15c)
        if unsafe { csr_read_raw::<0x15c>() } >> TOPEI_ID_SHIFT != 0 {
            unmask_irq(hartid);
            return false;
        }
        // 2. query m-mode irq pending (CSR_MTOPEI, 0x35c)
        if unsafe { csr_read_raw::<0x35c>() } >> TOPEI_ID_SHIFT != 0 {
            unmask_irq(hartid);
            return false;
        }
        // 3. query h-mode irq pending (CSR_HGEIP, 0xe12) -?X100 only
        if hartid < 8 && unsafe { csr_read_raw::<0xe12>() } != 0 {
            unmask_irq(hartid);
            return false;
        }
    }

    // Five clean rounds: vote core acpr + vote cluster3 power down
    vote_core_apcr(hartid);
    vote_powrdown_cluster(12);
    true
}

/// Performs the non-retentive suspend sequence of the current hart
/// (`__rpmi_hsm_suspend`, k3_corepm.c L685-795): save the IMSIC state,
/// vote power-down, disable prefetch/caches/snoop, `wfi`, then restore the
/// IMSIC state.
///
/// # Safety
///
/// The current hart must implement Smaia (and the hypervisor extension for
/// `hartid < 8`).
pub fn suspend(hartid: usize, sleep_type: u32, state: &mut ImsicConfig) {
    const SBI_HSM_SUSP_NON_RET_BIT: u32 = 1 << 0;
    const SBI_HSM_SUSP_PLAT_BASE: u32 = 0x10000000;

    mask_irq(hartid);

    // Save IMSIC state
    unsafe {
        imsic_save_machine_supervisor(state);
        if hartid < 8 {
            imsic_save_hypervisor(state);
        }
    }

    // Vote power-down (k3_corepm.c L760-772): hart 8/12 vote only their own
    // core; otherwise a platform suspend votes the core, a regular suspend
    // votes the whole cluster.
    if hartid == 8 || hartid == 12 {
        vote_powrdown_core(hartid);
    } else if sleep_type == (SBI_HSM_SUSP_NON_RET_BIT | SBI_HSM_SUSP_PLAT_BASE) {
        vote_powrdown_core(hartid);
    } else {
        vote_powrdown_cluster(hartid);
    }

    unsafe {
        // Disable prefetch, flush dcache, disable caches (csi_* helpers,
        // k3_corepm.c L774-785); emit fences as a best-effort approximation.
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

        // Disable core snoop (k3_corepm.c L784)
        csr_clear::<CSR_ML2SETUP>(1 << (hartid % PLATFORM_MAX_CPUS_PER_CLUSTER));
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

        // Wait for interrupt
        core::arch::asm!("wfi", options(nomem, nostack));

        // Re-enable core snoop (k3_corepm.c L793)
        csr_set::<CSR_ML2SETUP>(1 << (hartid % PLATFORM_MAX_CPUS_PER_CLUSTER));
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }

    // Restore IMSIC state
    unsafe {
        imsic_restore_machine_supervisor(state);
        if hartid < 8 {
            imsic_restore_hypervisor(state);
        }
    }

    unmask_irq(hartid);
}

/// Shutdown/power-down process for the current hart
/// (`__rpmi_shutdown_process`, k3_corepm.c L1023-1055): mask IRQs, vote
/// power-down, disable caches/snoop, and hang in `wfi`. Never returns.
pub fn shutdown_process(hartid: usize) -> ! {
    mask_irq(hartid);

    if hartid == 8 || hartid == 12 {
        vote_powrdown_core(hartid);
    } else {
        vote_powrdown_cluster(hartid);
    }
    vote_core_apcr(hartid);

    unsafe {
        // Disable local timer (k3_corepm.c L1036-1037)
        csr_write::<0x14d>(usize::MAX); // CSR_STIMECMP

        // Disable all IRQs (k3_corepm.c L1038-1039)
        csr_clear::<0x304>(MIP_ALL); // CSR_MIE

        // Disable prefetch/caches/snoop (k3_corepm.c L1040-1051)
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        csr_clear::<CSR_ML2SETUP>(1 << (hartid % PLATFORM_MAX_CPUS_PER_CLUSTER));
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }

    loop {
        unsafe { asm!("wfi", options(nomem, nostack)) };
    }
}

/// Configure the M-level APLIC (maplic @0xf1800000) to delegate the wired
/// IRQ range 1..=0x200 to the S-level APLIC (saplic), as required by the
/// K3 `riscv,delegate=<&saplic 0x1 0x200>` property.
///
/// On the K3 the device IRQs physically enter the root maplic; without this
/// delegation the interrupt signal stops at the maplic and never reaches the
/// saplic, so no MSI is sent to the S-mode IMSIC and no SEIP is raised for
/// Linux.
pub fn init_maplic_delegation() {
    if !crate::platform::IS_K3_PLATFORM.load(core::sync::atomic::Ordering::Acquire) {
        return;
    }
    /// M-level APLIC base (dts `interrupt-controller@f1800000`).
    const MAPLIC_BASE: usize = 0xf180_0000;
    const APLIC_DOMAINCFG: usize = 0x0000;
    const APLIC_SOURCECFG_BASE: usize = 0x0004;
    /// Delegation bit (aplic.h `APLIC_SOURCECFG_D`).
    const APLIC_SOURCECFG_D: u32 = 1 << 10;
    /// Child APLIC index for the saplic (first child = 0).
    const CHILD_INDEX_SAPLIC: u32 = 0;
    /// Delegate range from the K3 `riscv,delegate` property.
    const FIRST_DELEG_IRQ: u32 = 1;
    const LAST_DELEG_IRQ: u32 = 0x200;

    #[inline]
    fn wr(base: usize, off: usize, val: u32) {
        unsafe { core::ptr::write_volatile((base + off) as *mut u32, val) }
    }

    const APLIC_CLRIE_BASE: usize = 0x1f00;
    const APLIC_TARGET_BASE: usize = 0x3004;
    const APLIC_MMSICFGADDR: usize = 0x1bc0;
    const APLIC_MMSICFGADDRH: usize = 0x1bc4;
    const APLIC_SMSICFGADDR: usize = 0x1bc8;
    const APLIC_SMSICFGADDRH: usize = 0x1bcc;

    #[inline]
    fn rd(base: usize, off: usize) -> u32 {
        unsafe { core::ptr::read_volatile((base + off) as *const u32) }
    }

    // Cold-init sequence for the K3 maplic.
    // 1. Domain configuration to 0 (interrupt domain disabled).
    wr(MAPLIC_BASE, APLIC_DOMAINCFG, 0);
    // 2. Disable all interrupt enables (CLRIE, 32 sources per word).
    for i in (0..=LAST_DELEG_IRQ).step_by(32) {
        wr(
            MAPLIC_BASE,
            APLIC_CLRIE_BASE + (i as usize / 32) * 4,
            u32::MAX,
        );
    }
    // 3. Reset every source config and default priority, then delegate the
    //    whole wired range 1..=0x200 to the saplic (D | child_index).
    for i in FIRST_DELEG_IRQ..=LAST_DELEG_IRQ {
        let src_off = APLIC_SOURCECFG_BASE + (i - 1) as usize * 4;
        wr(MAPLIC_BASE, src_off, 0);
        wr(MAPLIC_BASE, APLIC_TARGET_BASE + (i - 1) as usize * 4, 1);
        wr(MAPLIC_BASE, src_off, APLIC_SOURCECFG_D | CHILD_INDEX_SAPLIC);
    }
    // 4. MSI target address configuration. Per the AIA spec these registers
    //    live only in the root APLIC and provide the MSI write addresses for
    //    the whole domain hierarchy: the saplic (MSI mode) composes its MSI
    //    target address from the ROOT maplic's smsicfgaddr/smsicfgaddrh.
    //    Without this the delegated interrupts are forwarded to the saplic,
    //    the saplic fires MSIs to an unprogrammed address, and the S-mode
    //    IMSIC never receives them (no SEIP for Linux).
    //    Values derived from the U-Boot DTB (k3_com260_ifx.dtb):
    //    - mimsic@f1000000: hart-index-bits=4, guest-index-bits=0
    //      -> mmsicfgaddr  = 0xf1000, mmsicfgaddrh = 0x4000 (LHXW=4)
    //    - simsic@e0400000: hart-index-bits=4, guest-index-bits=6,
    //      group-index-bits=0, group-index-shift=24
    //      -> smsicfgaddr  = 0xe0400, smsicfgaddrh = 0x604000 (LHXW=4, LHXS=6)
    //    Skip writes if the register pair is already locked (L bit set).
    let m_h = rd(MAPLIC_BASE, APLIC_MMSICFGADDRH);
    if m_h & (1 << 31) == 0 {
        wr(MAPLIC_BASE, APLIC_MMSICFGADDR, 0x000f_1000);
        wr(MAPLIC_BASE, APLIC_MMSICFGADDRH, 0x0000_4000);
    }
    let s_h = rd(MAPLIC_BASE, APLIC_SMSICFGADDRH);
    if s_h & (1 << 31) == 0 {
        wr(MAPLIC_BASE, APLIC_SMSICFGADDR, 0x000e_0400);
        wr(MAPLIC_BASE, APLIC_SMSICFGADDRH, 0x0060_4000);
    }
    info!(
        "[MAplic] delegated IRQ 1..=0x200 to saplic; msicfg m=0x{:x}/0x{:x} s=0x{:x}/0x{:x}",
        rd(MAPLIC_BASE, APLIC_MMSICFGADDR),
        rd(MAPLIC_BASE, APLIC_MMSICFGADDRH),
        rd(MAPLIC_BASE, APLIC_SMSICFGADDR),
        rd(MAPLIC_BASE, APLIC_SMSICFGADDRH)
    );
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
        // Vendor-compatible matching entries.
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
        assert_eq!(core::mem::size_of::<RO<u32>>(), 4);
        assert_eq!(core::mem::size_of::<RW<u32>>(), 4);
        assert_eq!(core::mem::size_of::<WO<u32>>(), 4);
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

    #[test]
    fn test_register_addresses() {
        // C0-C3 RVBADDR registers (k3.h L20-30).
        assert_eq!(C0_RVBADDR as usize, 0xd4282db0);
        assert_eq!(C1_RVBADDR as usize, 0xd4282eb0);
        assert_eq!(C2_RVBADDR as usize, 0xd4282fe8);
        assert_eq!(C3_RVBADDR as usize, 0xd4282e60);
        // PMU wakeup / idle cfg registers (k3.h L32-64).
        assert_eq!(PMU_CAP_CORE_WAKEUP[0] as usize, 0xd428292c);
        assert_eq!(PMU_CAP_CORE_WAKEUP[8] as usize, 0xd4282b60);
        assert_eq!(PMU_CAP_CORE_IDLE_CFG[8] as usize, 0xd4282b40);
        assert_eq!(PMU_CAP_CORE_IDLE_CFG[0] as usize, 0xd4282924);
        // APCR VETE registers (k3.h L83-98).
        assert_eq!(APCR_CORE_VETE_REG[0] as usize, 0xd40510c0);
        assert_eq!(APCR_CORE_VETE_REG[15] as usize, 0xd40510fc);
        // L2 flush control (k3.h L103-106).
        assert_eq!(PMU_C0_L2_FLUSH_CTRL as usize, 0xd84401b0);
        assert_eq!(PMU_C3_L2_FLUSH_CTRL as usize, 0xd84401ec);
        // DMASYS (k3.h L121-122).
        assert_eq!(DMASYS_RESET as usize, 0xd844022c);
        assert_eq!(DMASYS_CLK_EN as usize, 0xd8440234);
    }

    #[test]
    fn test_m_only_ranges() {
        // RVBADDR C0-C3 are M-only (spacemit_k3.c m_only_ranges[] L304-316).
        assert!(pa_is_m_only(0xd4282db0, 4));
        assert!(pa_is_m_only(0xd4282db4, 4));
        assert!(pa_is_m_only(0xd4282fe8, 8));
        // PMU_CAP_CORE8_IDLE_CFG..WAKEUP block (12 * 4 bytes).
        assert!(pa_is_m_only(0xd4282b40, 4));
        assert!(!pa_is_m_only(0xd4283000, 4)); // outside M-only ranges
    }

    #[test]
    fn test_imsic_config_layout() {
        // M-/S-mode EIE arrays hold 64 / 2 entries on 64-bit.
        assert_eq!(core::mem::size_of::<u64>(), 8);
        let cfg = ImsicConfig::default();
        assert_eq!(cfg.meie.len(), 32);
        assert_eq!(cfg.seie.len(), 32);
        assert_eq!(cfg.hc.len(), 8);
        let himsic = HimsicConfig::default();
        assert_eq!(himsic.heie.len(), 32);
        // IMSIC constants (k3.h L149-153).
        assert_eq!(IMSIC_EIDELIVERY, 0x70);
        assert_eq!(IMSIC_EITHRESHOLD, 0x72);
        assert_eq!(IMSIC_FIRST_EIE_REG, 0xc0);
        assert_eq!(MAX_IMSIC_EIE_REGISTERS, 64);
        assert_eq!(IMSIC_MAX_VGEN, 8);
    }
}
