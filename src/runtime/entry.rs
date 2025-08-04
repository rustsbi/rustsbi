pub type EfiMainFn =
    extern "efiapi" fn(image_handle: *mut core::ffi::c_void, system_table: *mut SystemTable) -> u64;

use core::mem::transmute;

use uefi_raw::table::system::SystemTable;

pub fn resolve_entry_func(mapping: *const u8, entry: u64, base_va: u64) -> EfiMainFn {
    let func_addr = (mapping as usize + (entry - base_va) as usize) as *const ();
    unsafe { transmute(func_addr) }
}
