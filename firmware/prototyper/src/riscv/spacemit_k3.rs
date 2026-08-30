//! SpacemiT K3 platform support.

use core::{arch::asm, cell::UnsafeCell};

const CSR_ML2SETUP: u16 = 0x7f0;
const CSR_ML2HINT: u16 = 0x7f7;
const CSR_PERF_CTRL: u16 = 0x7d0;
const CSR_PREFETCH_CTRL: u16 = 0x7d1;

// ML2SETUP bit fields (in addition to the per-hart L2 cache mask)
const ML2SETUP_IPRF: usize = 1 << 16; // Instruction prefetch enable
const ML2SETUP_TPRF: usize = 1 << 18; // TLB prefetch enable

// ML2HINT bit fields.
const ML2HINT_CIU_CHR2_MER_DIS: usize = 1 << 2; // Disable read/prefetch transaction merge
const ML2HINT_CIU_CHR2_DEPD_DIS: usize = 1 << 3; // Disable full address dependency check
const ML2HINT_TRACE_TOP_ICGEN: usize = 1 << 26; // RV-Trace top clock enable

// PERF_CTRL bit fields.
const PERF_CTRL_VEC_L1BYPASS: usize = 1 << 32; // Vector loads bypass L1, cached in L2 only

// PREFETCH_CTRL bit fields.
const PREFETCH_CTRL_L2_PERF_DIST: usize = 3 << 10; // L2 prefetch distance: 56 entries

#[repr(transparent)]
struct MmioReg(UnsafeCell<u32>);

// SAFETY: access is volatile and synchronization is defined by each device's
// register protocol rather than Rust memory accesses.
unsafe impl Sync for MmioReg {}

impl MmioReg {
    #[inline]
    fn read(&self) -> u32 {
        // SAFETY: `self` points at MMIO registers; reads are volatile so the
        // compiler cannot cache or reorder them.
        unsafe { self.0.get().read_volatile() }
    }

    #[inline]
    fn write(&self, val: u32) {
        // SAFETY: `self` points at MMIO registers; writes are volatile so
        // the compiler cannot elide or reorder them.
        unsafe { self.0.get().write_volatile(val) }
    }
}

#[inline]
fn mmio_reg(address: *const MmioReg) -> &'static MmioReg {
    // SAFETY: callers use fixed K3 register addresses after platform detection.
    unsafe { &*address }
}

#[inline]
fn warmboot_reg(address: *const WarmbootAddr) -> &'static WarmbootAddr {
    // SAFETY: callers use fixed K3 cluster RVBADDR addresses after detection.
    unsafe { &*address }
}

/// A pair of adjacent LO/HI warmboot address registers (8 bytes).
#[repr(C)]
struct WarmbootAddr {
    lo: MmioReg,
    hi: MmioReg,
}

impl WarmbootAddr {
    #[inline]
    fn set(&self, addr: u64) {
        self.lo.write(addr as u32);
        self.hi.write((addr >> 32) as u32);
    }
}

const C0_RVBADDR: *const WarmbootAddr = 0xd4282db0usize as *const WarmbootAddr;
const C1_RVBADDR: *const WarmbootAddr = (0xd4282c00usize + 0x2b0) as *const WarmbootAddr;
const C2_RVBADDR: *const WarmbootAddr = (0xd4282c00usize + 0x3e8) as *const WarmbootAddr;
const C3_RVBADDR: *const WarmbootAddr = (0xd4282c00usize + 0x260) as *const WarmbootAddr;

const PMU_CAP_BASE: usize = 0xd4282800;

const PMU_CAP_CORE_WAKEUP: [*const MmioReg; 16] = [
    (PMU_CAP_BASE + 0x12c) as *const MmioReg, // CORE0
    (PMU_CAP_BASE + 0x130) as *const MmioReg, // CORE1
    (PMU_CAP_BASE + 0x134) as *const MmioReg, // CORE2
    (PMU_CAP_BASE + 0x138) as *const MmioReg, // CORE3
    (PMU_CAP_BASE + 0x324) as *const MmioReg, // CORE4
    (PMU_CAP_BASE + 0x328) as *const MmioReg, // CORE5
    (PMU_CAP_BASE + 0x32c) as *const MmioReg, // CORE6
    (PMU_CAP_BASE + 0x330) as *const MmioReg, // CORE7
    (PMU_CAP_BASE + 0x360) as *const MmioReg, // CORE8
    (PMU_CAP_BASE + 0x364) as *const MmioReg, // CORE9
    (PMU_CAP_BASE + 0x368) as *const MmioReg, // CORE10
    (PMU_CAP_BASE + 0x36c) as *const MmioReg, // CORE11
    (PMU_CAP_BASE + 0x22c) as *const MmioReg, // CORE12
    (PMU_CAP_BASE + 0x230) as *const MmioReg, // CORE13
    (PMU_CAP_BASE + 0x234) as *const MmioReg, // CORE14
    (PMU_CAP_BASE + 0x238) as *const MmioReg, // CORE15
];

const PMU_CAP_CORE_IDLE_CFG: [*const MmioReg; 16] = [
    (PMU_CAP_BASE + 0x124) as *const MmioReg, // CORE0
    (PMU_CAP_BASE + 0x128) as *const MmioReg, // CORE1
    (PMU_CAP_BASE + 0x160) as *const MmioReg, // CORE2
    (PMU_CAP_BASE + 0x164) as *const MmioReg, // CORE3
    (PMU_CAP_BASE + 0x304) as *const MmioReg, // CORE4
    (PMU_CAP_BASE + 0x308) as *const MmioReg, // CORE5
    (PMU_CAP_BASE + 0x30c) as *const MmioReg, // CORE6
    (PMU_CAP_BASE + 0x310) as *const MmioReg, // CORE7
    (PMU_CAP_BASE + 0x340) as *const MmioReg, // CORE8
    (PMU_CAP_BASE + 0x344) as *const MmioReg, // CORE9
    (PMU_CAP_BASE + 0x348) as *const MmioReg, // CORE10
    (PMU_CAP_BASE + 0x34c) as *const MmioReg, // CORE11
    (PMU_CAP_BASE + 0x20c) as *const MmioReg, // CORE12
    (PMU_CAP_BASE + 0x210) as *const MmioReg, // CORE13
    (PMU_CAP_BASE + 0x214) as *const MmioReg, // CORE14
    (PMU_CAP_BASE + 0x218) as *const MmioReg, // CORE15
];

const PMU_CX_CAPMP_IDLE_CFG: [*const MmioReg; 16] = [
    (PMU_CAP_BASE + 0x120) as *const MmioReg, // CFG0  (cluster 0, hart 0)
    (PMU_CAP_BASE + 0xe4) as *const MmioReg,  // CFG1  (cluster 0, hart 1)
    (PMU_CAP_BASE + 0x150) as *const MmioReg, // CFG2  (cluster 0, hart 2)
    (PMU_CAP_BASE + 0x154) as *const MmioReg, // CFG3  (cluster 0, hart 3)
    (PMU_CAP_BASE + 0x314) as *const MmioReg, // CFG4  (cluster 1, hart 4)
    (PMU_CAP_BASE + 0x318) as *const MmioReg, // CFG5  (cluster 1, hart 5)
    (PMU_CAP_BASE + 0x31c) as *const MmioReg, // CFG6  (cluster 1, hart 6)
    (PMU_CAP_BASE + 0x320) as *const MmioReg, // CFG7  (cluster 1, hart 7)
    (PMU_CAP_BASE + 0x350) as *const MmioReg, // CFG8  (cluster 2, hart 8)
    (PMU_CAP_BASE + 0x354) as *const MmioReg, // CFG9  (cluster 2, hart 9)
    (PMU_CAP_BASE + 0x358) as *const MmioReg, // CFG10 (cluster 2, hart 10)
    (PMU_CAP_BASE + 0x35c) as *const MmioReg, // CFG11 (cluster 2, hart 11)
    (PMU_CAP_BASE + 0x21c) as *const MmioReg, // CFG12 (cluster 3, hart 12)
    (PMU_CAP_BASE + 0x220) as *const MmioReg, // CFG13 (cluster 3, hart 13)
    (PMU_CAP_BASE + 0x224) as *const MmioReg, // CFG14 (cluster 3, hart 14)
    (PMU_CAP_BASE + 0x228) as *const MmioReg, // CFG15 (cluster 3, hart 15)
];

const CPU_MASK_FI_INTTERUPT: u32 = (1 << 3) | (1 << 4);
const CPU_PWR_DOWN_VALUE: u32 = 0x1f;
const CLUSTER_PWR_DOWN_VALUE: u32 = 0x8f;

const APCR_CORE_VETE_REG: [*const MmioReg; 16] = [
    (0xd4050000usize + 0x10c0) as *const MmioReg, // CORE0
    (0xd4050000usize + 0x10c4) as *const MmioReg, // CORE1
    (0xd4050000usize + 0x10c8) as *const MmioReg, // CORE2
    (0xd4050000usize + 0x10cc) as *const MmioReg, // CORE3
    (0xd4050000usize + 0x10d0) as *const MmioReg, // CORE4
    (0xd4050000usize + 0x10d4) as *const MmioReg, // CORE5
    (0xd4050000usize + 0x10d8) as *const MmioReg, // CORE6
    (0xd4050000usize + 0x10dc) as *const MmioReg, // CORE7
    (0xd4050000usize + 0x10e0) as *const MmioReg, // CORE8
    (0xd4050000usize + 0x10e4) as *const MmioReg, // CORE9
    (0xd4050000usize + 0x10e8) as *const MmioReg, // CORE10
    (0xd4050000usize + 0x10ec) as *const MmioReg, // CORE11
    (0xd4050000usize + 0x10f0) as *const MmioReg, // CORE12
    (0xd4050000usize + 0x10f4) as *const MmioReg, // CORE13
    (0xd4050000usize + 0x10f8) as *const MmioReg, // CORE14
    (0xd4050000usize + 0x10fc) as *const MmioReg, // CORE15
];

const APCR_COREX_DEFAULT_VATE_VALUE: u32 = (1 << 3)
    | (1 << 13)
    | (1 << 14)
    | (1 << 19)
    | (1 << 25)
    | (1 << 26)
    | (1 << 27)
    | (1 << 29)
    | (1 << 31);

const PMU_L2_FLUSH_BASE: usize = 0xd8440000;
const PMU_C0_L2_FLUSH_CTRL: *const MmioReg = (PMU_L2_FLUSH_BASE + 0x1b0) as *const MmioReg;
const PMU_C1_L2_FLUSH_CTRL: *const MmioReg = (PMU_L2_FLUSH_BASE + 0x1b4) as *const MmioReg;
const PMU_C2_L2_FLUSH_CTRL: *const MmioReg = (PMU_L2_FLUSH_BASE + 0x1c4) as *const MmioReg;
const PMU_C3_L2_FLUSH_CTRL: *const MmioReg = (PMU_L2_FLUSH_BASE + 0x1ec) as *const MmioReg;
const PMU_L2_FLUSH_HW_TYPE: u32 = 1 << 0; // Hardware flush type
const PMU_L2_FLUSH_HW_EN: u32 = 1 << 2; // Hardware flush enable

const DMASYS_RESET: *const MmioReg = (PMU_L2_FLUSH_BASE + 0x22c) as *const MmioReg;
const DMASYS_CLK_EN: *const MmioReg = (PMU_L2_FLUSH_BASE + 0x234) as *const MmioReg;

// CCI-550 cache coherent interconnect

/// CCI-550 control and status registers.
#[repr(C)]
struct Cci550Registers {
    _ctrl_override: MmioReg, // 0x0000
    _reserved0: MmioReg,     // 0x0004
    _reserved1: MmioReg,     // 0x0008
    status: MmioReg,         // 0x000c
}

impl Cci550Registers {
    #[inline]
    fn change_pending(&self) -> bool {
        self.status.read() & CCI_550_STATUS_CHANGE_PENDING != 0
    }

    /// Registers of the slave interface at `idx`.
    ///
    /// # Safety
    ///
    /// `idx` must be a valid CCI-550 slave interface index (0..7).
    #[inline]
    unsafe fn slave_iface(&self, idx: usize) -> &CciSlaveIfaceRegisters {
        // SAFETY: `self` is a valid CCI-550 base address and `idx` is a valid
        // slave interface index, so the computed address aliases its MMIO.
        unsafe {
            &*((self as *const Self as usize + cci_slave_iface_offset(idx))
                as *const CciSlaveIfaceRegisters)
        }
    }
}

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

const PLATFORM_MAX_CPUS: usize = 8;
const PLATFORM_MAX_CPUS_PER_CLUSTER: usize = 4;
const fn cpu_to_cluster(cpu: usize) -> usize {
    cpu / PLATFORM_MAX_CPUS_PER_CLUSTER
}

#[inline]
unsafe fn csr_read<const CSR: u16>() -> usize {
    let r: usize;
    unsafe {
        asm!("csrr {r}, {csr}", r = out(reg) r, csr = const CSR, options(nomem));
    }
    r
}

#[inline]
unsafe fn csr_write<const CSR: u16>(val: usize) {
    unsafe {
        asm!("csrw {csr}, {val}", csr = const CSR, val = in(reg) val, options(nomem));
    }
}

#[inline]
unsafe fn csr_set<const CSR: u16>(bits: usize) {
    let old = unsafe { csr_read::<CSR>() };
    unsafe { csr_write::<CSR>(old | bits) };
}

#[inline]
unsafe fn csr_clear<const CSR: u16>(bits: usize) {
    let old = unsafe { csr_read::<CSR>() };
    unsafe { csr_write::<CSR>(old & !bits) };
}

fn vote_powrdown_core(hartid: usize) {
    let reg = mmio_reg(PMU_CAP_CORE_IDLE_CFG[hartid]);
    let value = reg.read() | CPU_PWR_DOWN_VALUE;
    reg.write(value);
}

fn vote_powrdown_cluster(hartid: usize) {
    let core_reg = mmio_reg(PMU_CAP_CORE_IDLE_CFG[hartid]);
    core_reg.write(core_reg.read() | CPU_PWR_DOWN_VALUE);
    let cluster_reg = mmio_reg(PMU_CX_CAPMP_IDLE_CFG[hartid]);
    cluster_reg.write(cluster_reg.read() | CLUSTER_PWR_DOWN_VALUE);
}

fn devote_pwrdown_cluster(hartid: usize) {
    let core_reg = mmio_reg(PMU_CAP_CORE_IDLE_CFG[hartid]);
    let value = core_reg.read() & !(CPU_PWR_DOWN_VALUE | CPU_MASK_FI_INTTERUPT);
    core_reg.write(value);
    let cluster_reg = mmio_reg(PMU_CX_CAPMP_IDLE_CFG[hartid]);
    cluster_reg.write(cluster_reg.read() & !CLUSTER_PWR_DOWN_VALUE);
}

fn vote_core_apcr(hartid: usize) {
    let reg = mmio_reg(APCR_CORE_VETE_REG[hartid]);
    reg.write(APCR_COREX_DEFAULT_VATE_VALUE);
}

fn devote_core_apcr(hartid: usize) {
    let reg = mmio_reg(APCR_CORE_VETE_REG[hartid]);
    reg.write(0);
}

/// Wakes a core from a PMU low-power state.
pub(crate) fn wakeup_core(hartid: usize) {
    let reg = mmio_reg(PMU_CAP_CORE_WAKEUP[hartid]);
    reg.write(1 << hartid);
}

/// Parks the first A100 core after preparing cluster 2 for warm boot.
fn boot_entry_dummy() -> ! {
    let hartid = crate::riscv::current_hartid();

    // SAFETY: this K3 A100 parking entry runs in M-mode on a hart with these
    // custom CSRs and fixed PMU register mappings.
    unsafe {
        csr_set::<CSR_PERF_CTRL>(PERF_CTRL_VEC_L1BYPASS);
        csr_set::<CSR_PREFETCH_CTRL>(PREFETCH_CTRL_L2_PERF_DIST);
        csr_clear::<CSR_ML2HINT>(ML2HINT_CIU_CHR2_DEPD_DIS);
        csr_set::<CSR_ML2HINT>(ML2HINT_CIU_CHR2_MER_DIS);

        // Keep the cluster powered until the parking state is prepared.
        devote_pwrdown_cluster(hartid);
        devote_core_apcr(hartid);

        // Future cluster 2 wakeups must use the normal HSM warm entry.
        warmboot_reg(C2_RVBADDR).set(warm_entry());

        vote_powrdown_core(8);

        csr_write::<0x14d>(usize::MAX); // CSR_STIMECMP

        const MIP_ALL: usize = (1 << 1) | (1 << 3) | (1 << 5) | (1 << 7) | (1 << 9) | (1 << 11);
        csr_clear::<0x304>(MIP_ALL); // CSR_MIE

        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

        csr_clear::<CSR_ML2SETUP>(1 << (hartid % PLATFORM_MAX_CPUS_PER_CLUSTER));
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }

    loop {
        // SAFETY: WFI runs after interrupts and power votes are configured.
        unsafe { asm!("wfi", options(nomem, nostack)) };
    }
}

/// Enables CCI-550 snoop and DVM messages on a slave interface.
///
/// # Safety
///
/// Must only be called once per interface during cold boot. `slave_if_id`
/// must be a valid CCI-550 slave interface index (0..6).
unsafe fn cci_enable_snoop_dvm_reqs(slave_if_id: usize) {
    // SAFETY: the function contract limits access to the K3 CCI-550 and a
    // valid slave interface.
    let cci = unsafe { &*CCI_550_BASE };
    let iface = unsafe { cci.slave_iface(slave_if_id) };

    iface
        .snoop_ctrl
        .write(CCI_550_SNOOP_CTRL_ENABLE_SNOOPS | CCI_550_SNOOP_CTRL_ENABLE_DVMS);

    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

    while cci.change_pending() {}
}

/// # Safety
///
/// Must only be called once during cold boot on the boot hart.
unsafe fn k3_pre_init(warmboot_addr: u64) {
    // RVBADDR must be visible before a cluster can be woken.
    for cluster in 0..4 {
        let (rvbaddr, l2_flush_ctrl) = match cluster {
            0 => (C0_RVBADDR, PMU_C0_L2_FLUSH_CTRL),
            1 => (C1_RVBADDR, PMU_C1_L2_FLUSH_CTRL),
            2 => (C2_RVBADDR, PMU_C2_L2_FLUSH_CTRL),
            3 => (C3_RVBADDR, PMU_C3_L2_FLUSH_CTRL),
            _ => unreachable!(),
        };
        warmboot_reg(rvbaddr).set(warmboot_addr);
        mmio_reg(l2_flush_ctrl).write(PMU_L2_FLUSH_HW_EN | PMU_L2_FLUSH_HW_TYPE);
    }

    for slave_if_id in 0..=6 {
        unsafe { cci_enable_snoop_dvm_reqs(slave_if_id) };
    }

    // Keep managed clusters powered, then use core 8 to install cluster 2's
    // normal warm entry before it parks.
    for hartid in 0..PLATFORM_MAX_CPUS {
        devote_pwrdown_cluster(hartid);
    }
    warmboot_reg(C2_RVBADDR).set(boot_entry_dummy as *const () as usize as u64);
    wakeup_core(8);

    // DMASYS must be clocked and out of reset before CPUs access the full TCM.
    mmio_reg(DMASYS_RESET).write(1);
    mmio_reg(DMASYS_CLK_EN).write(1);
}

/// # Safety
///
/// Cold initialization must run once in M-mode on the K3 boot hart.
pub unsafe fn early_init(cold_boot: bool, warmboot_addr: u64) {
    if cold_boot {
        unsafe { k3_pre_init(warmboot_addr) };
    }
}

pub fn cold_boot_init() {
    // SAFETY: platform detection selects this path once on the boot hart.
    unsafe { early_init(true, warm_entry()) }
}

/// Performs per-hart K3 setup and permits cold boot only on hart 0.
pub fn cold_boot_allowed(hart_id: usize) -> bool {
    let cluster_bit = 1 << (hart_id % PLATFORM_MAX_CPUS_PER_CLUSTER);
    // SAFETY: platform detection guarantees these K3 CSRs are present.
    unsafe {
        csr_set::<CSR_ML2SETUP>(cluster_bit | ML2SETUP_IPRF | ML2SETUP_TPRF);

        if hart_id >= 8 {
            csr_set::<CSR_PERF_CTRL>(PERF_CTRL_VEC_L1BYPASS);
            csr_set::<CSR_PREFETCH_CTRL>(PREFETCH_CTRL_L2_PERF_DIST);
            csr_clear::<CSR_ML2HINT>(ML2HINT_CIU_CHR2_DEPD_DIS);
            csr_set::<CSR_ML2HINT>(ML2HINT_CIU_CHR2_MER_DIS);
        }

        csr_set::<CSR_ML2HINT>(ML2HINT_TRACE_TOP_ICGEN);

        devote_pwrdown_cluster(hart_id);
        devote_core_apcr(hart_id);
    }
    hart_id == 0
}

unsafe extern "C" {
    #[link_name = "_start_warm_k3"]
    static START_WARM_K3: u8;
}

pub fn warm_entry() -> u64 {
    core::ptr::addr_of!(START_WARM_K3) as u64
}
#[inline]
pub const fn max_cpus() -> usize {
    PLATFORM_MAX_CPUS
}

#[inline]
pub const fn cpus_per_cluster() -> usize {
    PLATFORM_MAX_CPUS_PER_CLUSTER
}

#[inline]
pub fn is_k3_compatible(model: &str) -> bool {
    let lower = model.to_ascii_lowercase();
    lower.contains("k3")
        || lower.contains("pico-itx")
        || lower.contains("pico_itx")
        || lower.contains("com260")
        || lower.contains("fml13v05")
}

#[inline]
pub fn is_k3_platform<'a>(model: &str, compatibles: impl IntoIterator<Item = &'a str>) -> bool {
    let by_compatible = compatibles
        .into_iter()
        .any(|c| c.to_ascii_lowercase().starts_with("spacemit,k3"));
    by_compatible || is_k3_compatible(model)
}

pub const RCPU0_RUNTIME_SPACE_BASE_ADDR: usize = 0x100200000;
pub const RCPU0_RUNTIME_SPACE_SIZE: usize = 0x400000;
pub const RCPU1_RUNTIME_SPACE_BASE_ADDR: usize = 0x100800000;
pub const RCPU1_RUNTIME_SPACE_SIZE: usize = 0x400000;
pub const RCPU_DTB_SPACE_BASE_ADDR: usize = 0x100d00000;
pub const RCPU_DTB_SPACE_SIZE: usize = 0x100000;

/// K3 register window emulated by M-mode for S-mode accesses.
pub const REGISTER_PRESERVATION_BASE: usize = 0xd4282000;
pub const REGISTER_PRESERVATION_SIZE: usize = 0x1000;

const SATP64_MODE_SHIFT: usize = 60;
const SV39_VPN_BITS: usize = 9;
const SV39_VPN_MASK: usize = (1 << SV39_VPN_BITS) - 1;
const SV39_VPN2_SHIFT: usize = 30;
const SV39_VPN1_SHIFT: usize = 21;
const SV39_VPN0_SHIFT: usize = 12; // PAGE_SHIFT
const PTE_V: usize = 1 << 0; // valid
const PTE_PPN_SHIFT: usize = 10;

/// Translates an S-mode Sv39 virtual address for register emulation.
pub fn s_addr_to_pa(addr: usize) -> Option<usize> {
    // SAFETY: reading `satp` only observes the current hart's translation state.
    unsafe {
        let satp: usize;
        core::arch::asm!("csrr {}, satp", out(reg) satp, options(nomem));
        let mode = (satp >> SATP64_MODE_SHIFT) & 0xf;

        if mode == 0 {
            return Some(addr);
        }
        if mode != 8 {
            return None;
        }
        let upper = addr >> 39;
        let expected_upper = if addr & (1 << 38) == 0 {
            0
        } else {
            (1 << 25) - 1
        };
        if upper != expected_upper {
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
            if !crate::firmware::supervisor_writable(ptep, core::mem::size_of::<usize>()) {
                return None;
            }
            // SAFETY: `supervisor_writable` bounds the aligned PTE address to
            // supervisor RAM outside firmware and K3 protected windows.
            let pte = (ptep as *const usize).read_volatile();

            let readable = pte & (1 << 1) != 0;
            let writable = pte & (1 << 2) != 0;
            let executable = pte & (1 << 3) != 0;
            if pte & PTE_V == 0 || (!readable && writable) {
                return None;
            }

            ppn = (pte >> PTE_PPN_SHIFT) & ((1 << 44) - 1);

            if readable || executable {
                let pg_off_bits = 12 + SV39_VPN_BITS * (2 - i);
                let offset_mask = (1usize << pg_off_bits) - 1;
                let page_base = ppn << 12;
                if page_base & offset_mask != 0 {
                    return None;
                }
                return Some(page_base | (addr & offset_mask));
            }
        }
        None
    }
}

/// M-mode-only subranges of the emulated K3 register window.
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
    M_ONLY_RANGES.iter().any(|range| {
        range
            .base
            .checked_add(range.size)
            .is_some_and(|range_end| pa >= range.base && end <= range_end)
    })
}

/// Emulates a permitted S-mode load from the K3 register window.
pub fn emulate_load(addr: usize, len: usize) -> Option<u64> {
    let pa = s_addr_to_pa(addr)?;
    let end = pa.checked_add(len)?;
    let region_end = REGISTER_PRESERVATION_BASE.checked_add(REGISTER_PRESERVATION_SIZE)?;

    if !matches!(len, 1 | 2 | 4 | 8)
        || !pa.is_multiple_of(len)
        || pa < REGISTER_PRESERVATION_BASE
        || end > region_end
    {
        return None;
    }

    if pa_is_m_only(pa, len) {
        return None;
    }

    // SAFETY: `pa` is within the verified REGISTER_PRESERVATION window.
    Some(match len {
        1 => unsafe { (pa as *const u8).read_volatile() as u64 },
        2 => unsafe { (pa as *const u16).read_volatile() as u64 },
        4 => unsafe { (pa as *const u32).read_volatile() as u64 },
        8 => unsafe { (pa as *const u64).read_volatile() },
        _ => return None,
    })
}

/// Emulates a permitted S-mode store to the K3 register window.
pub fn emulate_store(addr: usize, len: usize, val: u64) -> bool {
    let pa = match s_addr_to_pa(addr) {
        Some(pa) => pa,
        None => return false,
    };

    let Some(end) = pa.checked_add(len) else {
        return false;
    };
    let Some(region_end) = REGISTER_PRESERVATION_BASE.checked_add(REGISTER_PRESERVATION_SIZE)
    else {
        return false;
    };
    if !matches!(len, 1 | 2 | 4 | 8)
        || !pa.is_multiple_of(len)
        || pa < REGISTER_PRESERVATION_BASE
        || end > region_end
    {
        return false;
    }

    if pa_is_m_only(pa, len) {
        return false;
    }

    // SAFETY: `pa` is within the verified REGISTER_PRESERVATION window.
    match len {
        1 => unsafe { (pa as *mut u8).write_volatile(val as u8) },
        2 => unsafe { (pa as *mut u16).write_volatile(val as u16) },
        4 => unsafe { (pa as *mut u32).write_volatile(val as u32) },
        8 => unsafe { (pa as *mut u64).write_volatile(val) },
        _ => return false,
    }
    true
}

const IMSIC_EIDELIVERY: usize = 0x70;
const IMSIC_EITHRESHOLD: usize = 0x72;
const IMSIC_FIRST_EIE_REG: usize = 0xc0;
const MAX_IMSIC_EIE_REGISTERS: usize = 64;
const IMSIC_MAX_VGEN: usize = 0x8;

const TOPEI_ID_SHIFT: usize = 16;

const CSR_MISELECT: usize = 0x350;
const CSR_MIREG: usize = 0x351;
const CSR_SISELECT: usize = 0x150;
const CSR_SIREG: usize = 0x151;
const CSR_VSISELECT: usize = 0x250;
const CSR_VSIREG: usize = 0x251;

const MIP_ALL: usize = (1 << 1) | (1 << 3) | (1 << 5) | (1 << 7) | (1 << 9) | (1 << 11);

/// IMSIC interrupt state saved across suspend. M- and S-mode state exists on
/// every managed hart; H/VS state exists only on X100 harts (`hartid < 8`).
#[derive(Clone, Copy, Default)]
pub struct ImsicConfig {
    pub meidelivery: usize,
    pub meithreshold: usize,
    pub meie: [usize; MAX_IMSIC_EIE_REGISTERS / 2],
    pub seidelivery: usize,
    pub seithreshold: usize,
    pub seie: [usize; MAX_IMSIC_EIE_REGISTERS / 2],
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
    pub hc: [HimsicConfig; IMSIC_MAX_VGEN],
}

#[derive(Clone, Copy, Default)]
pub struct HimsicConfig {
    pub heidelivery: usize,
    pub heithreshold: usize,
    pub heie: [usize; MAX_IMSIC_EIE_REGISTERS / 2],
}

/// Reads an indirect AIA CSR and restores the previous selector.
///
/// # Safety
///
/// The current hart must implement Smaia and be permitted to access the
/// corresponding privilege-level CSRs.
#[inline]
unsafe fn indirect_read<const SELECT: u16, const REG: u16>(reg_id: usize) -> usize {
    unsafe {
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
        core::arch::asm!(
            "csrw {sel}, {prev}",
            sel = const SELECT,
            prev = in(reg) prev,
            options(nomem),
        );
        value
    }
}

/// Writes an indirect AIA CSR and restores the previous selector.
///
/// # Safety
///
/// The current hart must implement Smaia and be permitted to access the
/// corresponding privilege-level CSRs.
#[inline]
unsafe fn indirect_write<const SELECT: u16, const REG: u16>(reg_id: usize, value: usize) {
    unsafe {
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
        core::arch::asm!(
            "csrw {sel}, {prev}",
            sel = const SELECT,
            prev = in(reg) prev,
            options(nomem),
        );
    }
}

/// Saves the M-/S-mode IMSIC interrupt-file state of the current hart.
///
/// # Safety
///
/// The current hart must implement Smaia.
pub unsafe fn imsic_save_machine_supervisor(state: &mut ImsicConfig) {
    unsafe {
        state.meithreshold =
            indirect_read::<{ CSR_MISELECT as u16 }, { CSR_MIREG as u16 }>(IMSIC_EITHRESHOLD);
        state.meidelivery =
            indirect_read::<{ CSR_MISELECT as u16 }, { CSR_MIREG as u16 }>(IMSIC_EIDELIVERY);
        for (i, sel) in (0..MAX_IMSIC_EIE_REGISTERS).step_by(2).enumerate() {
            state.meie[i] = indirect_read::<{ CSR_MISELECT as u16 }, { CSR_MIREG as u16 }>(
                IMSIC_FIRST_EIE_REG + sel,
            );
        }

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

/// Restores the M-/S-mode IMSIC interrupt-file state of the current hart.
///
/// # Safety
///
/// The current hart must implement Smaia.
pub unsafe fn imsic_restore_machine_supervisor(state: &ImsicConfig) {
    unsafe {
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

/// Saves the H/VS-mode IMSIC state of an X100 hart (`hartid < 8`).
///
/// # Safety
///
/// The current hart must implement Smaia and the hypervisor extension.
pub unsafe fn imsic_save_hypervisor(state: &mut ImsicConfig) {
    unsafe {
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

/// Restores the H/VS-mode IMSIC state of an X100 hart (`hartid < 8`).
///
/// # Safety
///
/// The current hart must implement Smaia and the hypervisor extension.
pub unsafe fn imsic_restore_hypervisor(state: &ImsicConfig) {
    unsafe {
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

/// # Safety
///
/// The CSR must be implemented on the current hart.
#[inline]
unsafe fn csr_write_raw<const CSR: u16>(value: usize) {
    unsafe {
        core::arch::asm!("csrw {csr}, {val}", csr = const CSR, val = in(reg) value, options(nomem));
    }
}

fn mask_irq(hartid: usize) {
    let reg = mmio_reg(PMU_CAP_CORE_IDLE_CFG[hartid]);
    reg.write(reg.read() | CPU_MASK_FI_INTTERUPT);
}

fn unmask_irq(hartid: usize) {
    let reg = mmio_reg(PMU_CAP_CORE_IDLE_CFG[hartid]);
    reg.write(reg.read() & !CPU_MASK_FI_INTTERUPT);
}

/// Prepares the current hart for suspend. Five consecutive IMSIC checks must
/// observe no M/S/VS interrupt before power votes are applied.
///
/// Returns `true` if the hart may proceed with suspend.
pub fn suspend_pre(hartid: usize) -> bool {
    mask_irq(hartid);

    let mut retry_count = 5usize;

    while retry_count != 0 {
        retry_count -= 1;

        // SAFETY: K3 harts implement the Smaia top-interrupt CSRs; X100 harts
        // also implement HGEIP.
        if unsafe { csr_read_raw::<0x15c>() } >> TOPEI_ID_SHIFT != 0 {
            unmask_irq(hartid);
            return false;
        }
        if unsafe { csr_read_raw::<0x35c>() } >> TOPEI_ID_SHIFT != 0 {
            unmask_irq(hartid);
            return false;
        }
        if hartid < 8 && unsafe { csr_read_raw::<0xe12>() } != 0 {
            unmask_irq(hartid);
            return false;
        }
    }

    vote_core_apcr(hartid);
    vote_powrdown_cluster(12);
    true
}

/// Performs the K3 non-retentive suspend sequence.
pub fn suspend(hartid: usize, sleep_type: u32, state: &mut ImsicConfig) {
    const SBI_HSM_SUSP_NON_RET_BIT: u32 = 1 << 0;
    const SBI_HSM_SUSP_PLAT_BASE: u32 = 0x10000000;

    mask_irq(hartid);

    // SAFETY: K3 harts implement Smaia, and X100 harts implement H-mode.
    unsafe {
        imsic_save_machine_supervisor(state);
        if hartid < 8 {
            imsic_save_hypervisor(state);
        }
    }

    // Harts 8 and 12 vote only their own core. Platform suspend also uses a
    // core vote; regular suspend votes the whole cluster.
    if hartid == 8 || hartid == 12 {
        vote_powrdown_core(hartid);
    } else if sleep_type == (SBI_HSM_SUSP_NON_RET_BIT | SBI_HSM_SUSP_PLAT_BASE) {
        vote_powrdown_core(hartid);
    } else {
        vote_powrdown_cluster(hartid);
    }

    // SAFETY: platform detection guarantees the K3 cache-control CSR.
    unsafe {
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

        csr_clear::<CSR_ML2SETUP>(1 << (hartid % PLATFORM_MAX_CPUS_PER_CLUSTER));
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

        core::arch::asm!("wfi", options(nomem, nostack));

        csr_set::<CSR_ML2SETUP>(1 << (hartid % PLATFORM_MAX_CPUS_PER_CLUSTER));
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }

    // SAFETY: the same hardware prerequisites apply to restoration.
    unsafe {
        imsic_restore_machine_supervisor(state);
        if hartid < 8 {
            imsic_restore_hypervisor(state);
        }
    }

    unmask_irq(hartid);
}

/// Powers down the current hart and waits indefinitely in `wfi`.
pub fn shutdown_process(hartid: usize) -> ! {
    mask_irq(hartid);

    if hartid == 8 || hartid == 12 {
        vote_powrdown_core(hartid);
    } else {
        vote_powrdown_cluster(hartid);
    }
    vote_core_apcr(hartid);

    // SAFETY: platform detection guarantees these K3 machine CSRs.
    unsafe {
        csr_write::<0x14d>(usize::MAX); // CSR_STIMECMP

        csr_clear::<0x304>(MIP_ALL); // CSR_MIE

        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        csr_clear::<CSR_ML2SETUP>(1 << (hartid % PLATFORM_MAX_CPUS_PER_CLUSTER));
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }

    loop {
        // SAFETY: the hart has completed the K3 shutdown sequence.
        unsafe { asm!("wfi", options(nomem, nostack)) };
    }
}

/// Delegates the K3 wired interrupt range from the M-level to S-level APLIC.
pub fn init_maplic_delegation() {
    if !crate::platform::IS_K3_PLATFORM.load(core::sync::atomic::Ordering::Acquire) {
        return;
    }
    const MAPLIC_BASE: usize = 0xf180_0000;
    const APLIC_DOMAINCFG: usize = 0x0000;
    const APLIC_SOURCECFG_BASE: usize = 0x0004;
    const APLIC_SOURCECFG_D: u32 = 1 << 10;
    const CHILD_INDEX_SAPLIC: u32 = 0;
    const FIRST_DELEG_IRQ: u32 = 1;
    const LAST_DELEG_IRQ: u32 = 0x200;

    #[inline]
    fn wr(base: usize, off: usize, val: u32) {
        // SAFETY: callers pass aligned offsets in the detected K3 APLIC.
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
        // SAFETY: callers pass aligned offsets in the detected K3 APLIC.
        unsafe { core::ptr::read_volatile((base + off) as *const u32) }
    }

    wr(MAPLIC_BASE, APLIC_DOMAINCFG, 0);
    for i in (0..=LAST_DELEG_IRQ).step_by(32) {
        wr(
            MAPLIC_BASE,
            APLIC_CLRIE_BASE + (i as usize / 32) * 4,
            u32::MAX,
        );
    }
    for i in FIRST_DELEG_IRQ..=LAST_DELEG_IRQ {
        let src_off = APLIC_SOURCECFG_BASE + (i - 1) as usize * 4;
        wr(MAPLIC_BASE, src_off, 0);
        wr(MAPLIC_BASE, APLIC_TARGET_BASE + (i - 1) as usize * 4, 1);
        wr(MAPLIC_BASE, src_off, APLIC_SOURCECFG_D | CHILD_INDEX_SAPLIC);
    }
    // The AIA root APLIC owns both IMSIC address configurations. Preserve a
    // firmware-locked configuration and otherwise use the K3 DT encodings.
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
        assert!(is_k3_compatible("SpacemiT K3 Pico-ITX"));
        assert!(is_k3_compatible("SpacemiT K3 CoM260 Module"));
        assert!(is_k3_compatible("SpacemiT K3 CoM260 IFX"));
        assert!(is_k3_compatible("DeepComputing FML13V05"));
        assert!(!is_k3_compatible("OrangePi RV2"));
        assert!(!is_k3_compatible("sifive,fu740"));
    }

    #[test]
    fn test_platform_detection() {
        assert!(is_k3_platform("", ["spacemit,k3"]));
        assert!(is_k3_platform("SpacemiT K3", ["riscv-spacemit"]));
        assert!(!is_k3_platform("", ["riscv-spacemit"]));
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
        assert!(is_k3_platform("SpacemiT K3 Pico-ITX", ["sifive,fu740"]));
        assert!(!is_k3_platform("OrangePi RV2", ["spacemit,k1"]));
        assert!(!is_k3_platform("Sifive FU740", ["sifive,fu740"]));
    }

    #[test]
    fn test_register_layout() {
        assert_eq!(core::mem::size_of::<MmioReg>(), 4);
        assert_eq!(offset_of!(WarmbootAddr, lo), 0x0);
        assert_eq!(offset_of!(WarmbootAddr, hi), 0x4);
        assert_eq!(core::mem::size_of::<WarmbootAddr>(), 8);
        assert_eq!(offset_of!(Cci550Registers, status), 0x000c);
        assert_eq!(offset_of!(CciSlaveIfaceRegisters, snoop_ctrl), 0x0);
        assert_eq!(cci_slave_iface_offset(0), 0x1000);
        assert_eq!(cci_slave_iface_offset(3), 0x4000);
    }

    #[test]
    fn test_register_addresses() {
        assert_eq!(C0_RVBADDR as usize, 0xd4282db0);
        assert_eq!(C1_RVBADDR as usize, 0xd4282eb0);
        assert_eq!(C2_RVBADDR as usize, 0xd4282fe8);
        assert_eq!(C3_RVBADDR as usize, 0xd4282e60);
        assert_eq!(PMU_CAP_CORE_WAKEUP[0] as usize, 0xd428292c);
        assert_eq!(PMU_CAP_CORE_WAKEUP[8] as usize, 0xd4282b60);
        assert_eq!(PMU_CAP_CORE_IDLE_CFG[8] as usize, 0xd4282b40);
        assert_eq!(PMU_CAP_CORE_IDLE_CFG[0] as usize, 0xd4282924);
        assert_eq!(APCR_CORE_VETE_REG[0] as usize, 0xd40510c0);
        assert_eq!(APCR_CORE_VETE_REG[15] as usize, 0xd40510fc);
        assert_eq!(PMU_C0_L2_FLUSH_CTRL as usize, 0xd84401b0);
        assert_eq!(PMU_C3_L2_FLUSH_CTRL as usize, 0xd84401ec);
        assert_eq!(DMASYS_RESET as usize, 0xd844022c);
        assert_eq!(DMASYS_CLK_EN as usize, 0xd8440234);
    }

    #[test]
    fn test_m_only_ranges() {
        assert!(pa_is_m_only(0xd4282db0, 4));
        assert!(pa_is_m_only(0xd4282db4, 4));
        assert!(pa_is_m_only(0xd4282fe8, 8));
        assert!(pa_is_m_only(0xd4282b40, 4));
        assert!(!pa_is_m_only(0xd4283000, 4)); // outside M-only ranges
    }

    #[test]
    fn test_imsic_config_layout() {
        assert_eq!(core::mem::size_of::<u64>(), 8);
        let cfg = ImsicConfig::default();
        assert_eq!(cfg.meie.len(), 32);
        assert_eq!(cfg.seie.len(), 32);
        assert_eq!(cfg.hc.len(), 8);
        let himsic = HimsicConfig::default();
        assert_eq!(himsic.heie.len(), 32);
        assert_eq!(IMSIC_EIDELIVERY, 0x70);
        assert_eq!(IMSIC_EITHRESHOLD, 0x72);
        assert_eq!(IMSIC_FIRST_EIE_REG, 0xc0);
        assert_eq!(MAX_IMSIC_EIE_REGISTERS, 64);
        assert_eq!(IMSIC_MAX_VGEN, 8);
    }
}
