use aclint::SifiveClint;
use core::{
    arch::asm,
    ptr::null_mut,
    sync::atomic::{
        AtomicPtr,
        Ordering::{Relaxed, Release},
    },
};
use rustsbi::SbiRet;

pub(crate) static SIFIVECLINT: AtomicPtr<SifiveClint> = AtomicPtr::new(null_mut());

pub(crate) fn init(base: usize) {
    SIFIVECLINT.store(base as _, Release);
}

#[inline]
pub fn clear() {
    let hart_id = riscv::register::mhartid::read();
    loop {
        if let Some(clint) = unsafe { SIFIVECLINT.load(Relaxed).as_ref() } {
            clint.clear_msip(hart_id);
            clint.write_mtimecmp(hart_id, u64::MAX);
            break;
        } else {
            continue;
        }
    }
}

#[inline]
pub fn clear_msip() {
    let hart_id = riscv::register::mhartid::read();
    loop {
        if let Some(clint) = unsafe { SIFIVECLINT.load(Relaxed).as_ref() } {
            clint.clear_msip(hart_id);
            break;
        } else {
            continue;
        }
    }
}

#[inline]
pub fn set_msip(hart_id: usize) {
    loop {
        if let Some(clint) = unsafe { SIFIVECLINT.load(Relaxed).as_ref() } {
            clint.set_msip(hart_id);
            break;
        } else {
            continue;
        }
    }
}

pub struct ClintDevice<'a> {
    pub clint: &'a AtomicPtr<SifiveClint>,
    pub max_hart_id: usize,
}

impl<'a> ClintDevice<'a> {
    pub fn new(clint: &'a AtomicPtr<SifiveClint>, max_hart_id: usize) -> Self {
        Self { clint, max_hart_id }
    }
}

impl<'a> ClintDevice<'a> {
    #[inline]
    pub fn get_time(&self) -> usize {
        unsafe { (*self.clint.load(Relaxed)).read_mtime() as u32 as usize }
    }

    #[inline]
    pub fn get_timeh(&self) -> usize {
        unsafe { ((*self.clint.load(Relaxed)).read_mtime() >> 32) as usize }
    }
}

impl<'a> rustsbi::Timer for ClintDevice<'a> {
    #[inline]
    fn set_timer(&self, stime_value: u64) {
        unsafe {
            // TODO: 添加CPU拓展探测机制，补充无Sstc拓展时的定时器设置
            asm!(
                "csrrs zero, stimecmp, {}",
                in(reg) stime_value
            );
            riscv::register::mie::set_mtimer();
            // (*self.clint.load(Relaxed)).write_mtimecmp(current_hart_id, stime_value);
        }
    }
}

impl<'a> rustsbi::Ipi for ClintDevice<'a> {
    #[inline]
    fn send_ipi(&self, hart_mask: rustsbi::HartMask) -> SbiRet {
        for hart_id in 0..=self.max_hart_id {
            if hart_mask.has_bit(hart_id) {
                unsafe {
                    (*self.clint.load(Relaxed)).set_msip(hart_id);
                }
            }
        }
        SbiRet::success(0)
    }
}
