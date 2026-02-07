pub type EfiMainFn =
    extern "efiapi" fn(image_handle: *mut core::ffi::c_void, system_table: *mut SystemTable) -> u64;

use core::mem::transmute;

use uefi_raw::table::system::SystemTable;

/// Resolve the entry function from an RVA (relative virtual address).
///
/// For PE/COFF, the entry point in the optional header is an RVA, and
/// `object::Object::entry()` returns `image_base + entry_rva`.
pub fn resolve_entry_func(mapping: *const u8, entry_rva: u64) -> EfiMainFn {
    let func_addr = (mapping as usize + entry_rva as usize) as *const ();
    unsafe { transmute(func_addr) }
}
