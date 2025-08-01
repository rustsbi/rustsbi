use axsync::Mutex;
use lazyinit::LazyInit;
use uefi_raw::{Guid, Handle};

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(transparent)]
pub struct HandleWrapper(pub Handle);

unsafe impl Send for HandleWrapper {}
unsafe impl Sync for HandleWrapper {}

static HANDLES: LazyInit<Mutex<BTreeMap<HandleWrapper, Vec<Guid>>>> = LazyInit::new();

pub fn init_handle_service() {
    HANDLES.init_once(Mutex::new(BTreeMap::new()));
}

pub fn register_handle(handle: Handle, protocols: Vec<Guid>) {
    let mut handles = HANDLES.lock();
    handles
        .entry(HandleWrapper(handle))
        .or_insert_with(Vec::new)
        .extend(protocols);
}

pub fn unregister_handle(handle: Handle) {
    let mut handles = HANDLES.lock();
    handles.remove(&HandleWrapper(handle));
}

pub fn get_protocols(handle: Handle) -> Option<Vec<Guid>> {
    let handles = HANDLES.lock();
    handles.get(&HandleWrapper(handle)).cloned()
}

pub fn get_all_handles() -> Vec<Handle> {
    let handles = HANDLES.lock();
    handles.keys().map(|h| h.0).collect()
}
