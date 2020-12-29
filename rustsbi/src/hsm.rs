use crate::ecall::SbiRet;

/// Hart State Management Extension
pub trait Hsm: Send {
    fn hart_start(&mut self, hartid: usize, start_addr: usize, priv_: usize) -> SbiRet;

    fn hart_stop(&mut self, hartid: usize) -> SbiRet;

    fn hart_get_status(&self, hartid: usize) -> SbiRet;
}

use alloc::boxed::Box;
use spin::Mutex;

lazy_static::lazy_static! {
    static ref HSM: Mutex<Option<Box<dyn Hsm>>> =
        Mutex::new(None);
}

#[doc(hidden)] // use through a macro or a call from implementation
pub fn init_hsm<T: Hsm + Send + 'static>(hsm: T) {
    *HSM.lock() = Some(Box::new(hsm));
}

#[inline]
pub(crate) fn probe_hsm() -> bool {
    HSM.lock().as_ref().is_some()
}

pub(crate) fn hart_start(hartid: usize, start_addr: usize, priv_: usize) -> SbiRet {
    if let Some(obj) = &mut *HSM.lock() {
        return obj.hart_start(hartid, start_addr, priv_);
    }
    SbiRet::not_supported()
}

pub(crate) fn hart_stop(hartid: usize) -> SbiRet {
    if let Some(obj) = &mut *HSM.lock() {
        return obj.hart_stop(hartid);
    }
    SbiRet::not_supported()
}

pub(crate) fn hart_get_status(hartid: usize) -> SbiRet {
    if let Some(obj) = &mut *HSM.lock() {
        return obj.hart_get_status(hartid);
    }
    SbiRet::not_supported()
}
