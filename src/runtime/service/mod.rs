use axsync::Mutex;
use lazyinit::LazyInit;
use uefi_raw::table::{boot::BootServices, runtime::RuntimeServices};

use crate::runtime::service::{boot_service::Boot, runtime_service::Runtime};

pub(crate) mod boot_service;
pub(crate) mod memory;
pub(crate) mod runtime_service;

static BOOT_SERVICE: LazyInit<Mutex<Boot>> = LazyInit::new();
static RUNTIME_SERVICE: LazyInit<Mutex<Runtime>> = LazyInit::new();

pub(crate) fn init_service() {
    BOOT_SERVICE.init_once(Mutex::new(Boot::new()));
    RUNTIME_SERVICE.init_once(Mutex::new(Runtime::new()));
}

pub fn get_boot_service() -> *mut BootServices {
    BOOT_SERVICE
        .get()
        .expect("BootService not initialized")
        .lock()
        .get_services()
}

pub fn get_runtime_service() -> *mut RuntimeServices {
    RUNTIME_SERVICE
        .get()
        .expect("RuntimeService not initialized")
        .lock()
        .get_services()
}
