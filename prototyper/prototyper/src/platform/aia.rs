use core::sync::atomic::{AtomicBool, Ordering};
use riscv_aia::Iid;
use riscv_aia::peripheral::imsic::system::AddressLayout;

use crate::cfg::NUM_HART_MAX;
use crate::riscv::csr::stimecmp;
use crate::riscv::current_hartid;
use crate::sbi::ipi::IpiDevice;

pub(crate) const IMSIC_COMPATIBLE: [&str; 2] = ["riscv,imsics", "riscv,imsic"];

static AIA_ACTIVE: AtomicBool = AtomicBool::new(false);

const QEMU_VIRT_M_APLIC_BASE: usize = 0x0c00_0000;
const QEMU_VIRT_S_IMSIC_BASE: usize = 0x2800_0000;
const QEMU_VIRT_APLIC_NUM_SOURCES: usize = 0x60;
const APLIC_DOMAINCFG: usize = 0x0000;
const APLIC_SOURCECFG_BASE: usize = 0x0004;
const APLIC_MMSICFGADDR: usize = 0x1bc0;
const APLIC_MMSICFGADDRH: usize = 0x1bc4;
const APLIC_SMSICFGADDR: usize = 0x1bc8;
const APLIC_SMSICFGADDRH: usize = 0x1bcc;
const APLIC_CLRIE_BASE: usize = 0x1f00;
const APLIC_SOURCECFG_DELEGATE: u32 = 1 << 10;
const APLIC_MSICFGADDRH_LOCK: u32 = 1 << 31;
const APLIC_MSICFGADDRH_LHXW_SHIFT: u32 = 12;

pub fn is_aia_active() -> bool {
    AIA_ACTIVE.load(Ordering::Relaxed)
}

pub fn set_aia_active(active: bool) {
    AIA_ACTIVE.store(active, Ordering::Relaxed);
}

pub fn init_qemu_m_aplic_delegation(machine_imsic_base: usize, hart_index_bits: u32) {
    let base = QEMU_VIRT_M_APLIC_BASE;

    write_aplic(base + APLIC_DOMAINCFG, 0);

    for source in (0..=QEMU_VIRT_APLIC_NUM_SOURCES).step_by(32) {
        write_aplic(base + APLIC_CLRIE_BASE + (source / 32) * 4, u32::MAX);
    }

    for source in 1..=QEMU_VIRT_APLIC_NUM_SOURCES {
        write_aplic(
            base + APLIC_SOURCECFG_BASE + (source - 1) * 4,
            APLIC_SOURCECFG_DELEGATE,
        );
    }

    if read_aplic(base + APLIC_MMSICFGADDRH) & APLIC_MSICFGADDRH_LOCK == 0 {
        write_msicfg(
            base + APLIC_MMSICFGADDR,
            base + APLIC_MMSICFGADDRH,
            machine_imsic_base,
            hart_index_bits,
        );
        write_msicfg(
            base + APLIC_SMSICFGADDR,
            base + APLIC_SMSICFGADDRH,
            QEMU_VIRT_S_IMSIC_BASE,
            hart_index_bits,
        );
    } else {
        warn!("AIA: M-level APLIC MSI configuration is locked");
    }

    info!(
        "AIA: delegated M-level APLIC IRQs 1..={} to S-level child",
        QEMU_VIRT_APLIC_NUM_SOURCES
    );
}

fn write_msicfg(addr: usize, addrh: usize, imsic_base: usize, hart_index_bits: u32) {
    let mut base_ppn = imsic_base >> 12;
    base_ppn &= !((1usize << hart_index_bits) - 1);

    write_aplic(addr, base_ppn as u32);
    write_aplic(
        addrh,
        ((base_ppn >> 32) as u32) | (hart_index_bits << APLIC_MSICFGADDRH_LHXW_SHIFT),
    );
}

fn read_aplic(addr: usize) -> u32 {
    unsafe { (addr as *const u32).read_volatile() }
}

fn write_aplic(addr: usize, value: u32) {
    unsafe {
        (addr as *mut u32).write_volatile(value);
    }
}

pub struct AiaInfo {
    pub layout: AddressLayout,
    pub num_ids: u16,
    pub firmware_ipi_iid: Iid,
    pub hart_imsic_map: [Option<usize>; NUM_HART_MAX],
}

pub struct ImsicDevice {
    firmware_ipi_iid: Iid,
    hart_imsic_map: [Option<usize>; NUM_HART_MAX],
}

unsafe impl Send for ImsicDevice {}

impl ImsicDevice {
    pub fn new(firmware_ipi_iid: Iid, hart_imsic_map: [Option<usize>; NUM_HART_MAX]) -> Self {
        Self {
            firmware_ipi_iid,
            hart_imsic_map,
        }
    }
}

impl IpiDevice for ImsicDevice {
    #[inline(always)]
    fn read_mtime(&self) -> u64 {
        riscv::register::time::read64()
    }

    #[inline(always)]
    fn write_mtime(&self, _val: u64) {}

    #[inline(always)]
    fn read_mtimecmp(&self, _hart_idx: usize) -> u64 {
        0
    }

    #[inline(always)]
    fn write_mtimecmp(&self, hart_idx: usize, val: u64) {
        if hart_idx == current_hartid() {
            stimecmp::set(val);
            if val == u64::MAX {
                unsafe {
                    riscv::register::mip::clear_stimer();
                    riscv::register::mie::clear_mtimer();
                }
            }
        }
    }

    #[inline(always)]
    fn read_msip(&self, _hart_idx: usize) -> bool {
        false
    }

    #[inline(always)]
    fn set_msip(&self, hart_id: usize) {
        let Some(addr) = self.hart_imsic_map.get(hart_id).copied().flatten() else {
            warn!("IMSIC ring: hart {} has no mapped IMSIC file", hart_id);
            return;
        };
        core::sync::atomic::fence(core::sync::atomic::Ordering::Release);
        let data = riscv_aia::peripheral::imsic::msi::encode_le(self.firmware_ipi_iid.number());
        unsafe {
            (addr as *mut u32).write_volatile(data);
        }
    }

    #[inline(always)]
    fn clear_msip(&self, _hart_idx: usize) {
        let _ = mtopei_claim();
    }
}

pub(crate) fn mtopei_claim() -> Option<Iid> {
    let bits: usize;
    unsafe {
        core::arch::asm!(
            "csrrw {val}, 0x35C, zero",
            val = out(reg) bits,
        );
    }
    let iid_bits = ((bits & 0x0FFF_0000) >> 16) as u16;
    Iid::new(iid_bits)
}

pub fn imsic_init_hart(info: &AiaInfo) {
    let ipi_iid = info.firmware_ipi_iid.number();
    let num_ids = info.num_ids;

    imsic_write_indirect(0x70, 1);
    imsic_write_indirect(0x72, 0);

    let max_reg = ((num_ids as usize) + 31) / 32;
    for i in 0..max_reg {
        #[cfg(target_pointer_width = "64")]
        if i % 2 == 1 {
            continue;
        }
        let eip_sel = 0x80 + i;
        let eie_sel = 0xC0 + i;
        imsic_write_indirect(eip_sel, 0);
        imsic_write_indirect(eie_sel, 0);
    }

    {
        let iid = ipi_iid as usize;
        #[cfg(target_pointer_width = "64")]
        let eie_sel = 0xC0 + (iid / 64) * 2;
        #[cfg(target_pointer_width = "32")]
        let eie_sel = 0xC0 + iid / 32;
        let bit_pos = iid % (core::mem::size_of::<usize>() * 8);
        let current = imsic_read_indirect(eie_sel);
        imsic_write_indirect(eie_sel, current | (1usize << bit_pos));
    }

    unsafe {
        riscv::register::mie::set_mext();
    }
    debug!(
        "IMSIC: hart init done, MEIE enabled, firmware IPI IID={}",
        ipi_iid
    );
}

fn imsic_write_indirect(select: usize, value: usize) {
    unsafe {
        core::arch::asm!(
            "csrw 0x350, {sel}",
            "csrw 0x351, {val}",
            sel = in(reg) select,
            val = in(reg) value,
        );
    }
}

fn imsic_read_indirect(select: usize) -> usize {
    let value: usize;
    unsafe {
        core::arch::asm!(
            "csrw 0x350, {sel}",
            "csrr {val}, 0x351",
            sel = in(reg) select,
            val = out(reg) value,
        );
    }
    value
}
