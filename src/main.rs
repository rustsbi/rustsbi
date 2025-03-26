#![cfg_attr(feature = "axstd", no_std)]
#![cfg_attr(feature = "axstd", no_main)]

#[macro_use]
#[cfg(feature = "axstd")]
extern crate axstd as std;

use axdriver::AxDeviceContainer;
use axdriver_block::ramdisk::RamDisk;
use axfs::api as fs;
use fs::File;
use object::{Object, ObjectSection};

const IMG_PATH: &'static str = "target/arceboot.img";

fn make_disk() -> std::io::Result<RamDisk> {
    let path = std::env::current_dir()?.join(IMG_PATH);
    println!("Loading disk image from {:?} ...", path);
    let data = std::fs::read(path)?;
    println!("size = {} bytes", data.len());
    Ok(RamDisk::from(&data))
}

#[cfg_attr(feature = "axstd", unsafe(no_mangle))]
fn main() {
    println!("opening arceboot.img...");
    let disk = make_disk().expect("failed to load disk image");
    axfs::init_filesystems(AxDeviceContainer::from_one(disk));
    let fname = "/EFI/BOOT/BOOTRISCV64.EFI";
    println!("Reading fname: {}", fname);
    let file = File::options().read(true).write(true).open(fname).unwrap();
    let file_size = file.metadata().unwrap().len();
    println!("size = {}", file_size);
    let binary_data = fs::read(fname).unwrap();
    let efi = object::File::parse(&*binary_data).unwrap();
    for section in efi.sections() {
        println!("{}", section.name().unwrap());
    }
    println!("finished opening arceboot.img...");
    
    // ... 加载流程
    
    // address: usize
    // let entry = mem_address as Entry;
    //
    // let status = entry();
    // if status.is_error() {
    //     println!("...")
    // }
}

// efi_main (EFI_HANDLE ImageHandle, EFI_SYSTEM_TABLE *SystemTable)
//type Entry = extern "efiapi" fn(uefi::Handle, *const core::ffi::c_void) -> uefi::Status;

// extern "efiapi" fn output_string(this: *mut SimpleTextOutputProtocol, string: *const Char16) -> Status {
//
// }
